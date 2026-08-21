//! Capture-key interception, backed by the `kbd-global` global hotkey
//! runtime.
//!
//! The capture key is `Ctrl` + `Super` + `Space`: a base key (`Space`) held
//! together with the `Ctrl` and `Super` modifiers. A modifier-only
//! combination has no base key for `kbd-global` to bind to, so it never
//! triggers a hotkey callback; giving the combination a base key makes it a
//! real hotkey. `kbd-global` owns evdev device discovery, hotplug, and the
//! event loop, and (in grab mode) forwards unmatched events through a virtual
//! `uinput` device so the desktop keeps working.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use kbd::hotkey::Hotkey;
use kbd_global::manager::HotkeyManager;

/// How often the held key state is polled to detect the capture release.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(20);

/// Runs the capture-key interception until the process is terminated.
///
/// Builds a `kbd-global` [`HotkeyManager`] in grab mode (exclusive capture
/// with `uinput` forwarding, so the desktop keeps working) and registers
/// `capture_key` as a hotkey. The registered callback fires on the key-down
/// edge, so the press is detected without polling latency and the combination
/// is consumed rather than forwarded to the desktop. The loop then polls the
/// held key state to detect the release — the callback only reports presses —
/// and doubles as a fallback that detects the press if the callback does not
/// fire. A press is logged on the inactive -> active transition and a release
/// on the active -> inactive transition.
pub fn run(capture_key: Hotkey) -> Result<(), Box<dyn std::error::Error>> {
    // Keep the manager alive so its engine thread runs the event loop.
    // Grab mode forwards unmatched events to the desktop, so typing works.
    let manager = HotkeyManager::builder().grab().build()?;

    // Shared with the hotkey callback so the callback and the poll below agree
    // on whether the capture is currently active, and each transition is
    // logged exactly once no matter which of the two observes it first.
    let active = Arc::new(AtomicBool::new(false));

    // Bind the guard to a named variable: dropping it unregisters the hotkey.
    let callback_active = Arc::clone(&active);
    let _binding = manager.register(capture_key, move || {
        if !callback_active.swap(true, Ordering::SeqCst) {
            log::info!("capture_key: pressed");
        }
    })?;

    log::info!("capture_key: listening for {capture_key}");

    loop {
        if capture_held(&manager, capture_key) {
            if !active.swap(true, Ordering::SeqCst) {
                log::info!("capture_key: pressed");
            }
        } else if active.swap(false, Ordering::SeqCst) {
            log::info!("capture_key: released");
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Whether the capture combination is currently held.
///
/// The capture is held when every modifier of `capture_key` is held and its
/// base key is pressed, so releasing either the base key or any one modifier
/// ends the capture. A stopped engine reports the capture as not held, which
/// releases an in-progress capture rather than leaving it stuck active.
fn capture_held(manager: &HotkeyManager, capture_key: Hotkey) -> bool {
    let Ok(modifiers) = manager.active_modifiers() else {
        return false;
    };

    capture_key.modifiers().all(|m| modifiers.contains(m))
        && manager.is_key_pressed(capture_key.key()).unwrap_or(false)
}

/// The capture key combination, expressed with `kbd` types.
///
/// A `kbd` hotkey is a base key combined with modifiers, so the capture
/// combination needs a base key to trigger the callback on. `kbd-global`
/// fires the callback when a non-modifier key is pressed with the required
/// modifiers held, so the capture combination is `Ctrl` + `Super` + `Space`:
/// the base key is `Space`, and `Ctrl` + `Super` are the modifiers. The
/// evdev layer reports the Super/Win key as the Meta key, which `kbd`
/// names the `Super` modifier.
pub fn capture_hotkey() -> Hotkey {
    "Ctrl+Super+Space".parse().expect("valid capture hotkey")
}

#[cfg(test)]
mod tests {
    use super::*;

    use kbd::hotkey::Modifier;
    use kbd::key::Key;

    /// Guards the `expect` in [`capture_hotkey`]: the combination has to parse,
    /// and it has to parse into `Space` plus the `Ctrl` and `Super` modifiers.
    #[test]
    fn capture_hotkey_is_space_with_ctrl_and_super() {
        let hotkey = capture_hotkey();

        assert_eq!(hotkey.key(), Key::SPACE);
        assert!(hotkey.has_modifier(Modifier::Ctrl));
        assert!(hotkey.has_modifier(Modifier::Super));
        assert_eq!(hotkey.modifier_count(), 2);
    }
}
