## Context

`steno-daemon` is a long-running Rust process (`crates/steno-daemon`) that, per the architecture docs, owns audio capture, transcription, and input injection. Its `main.rs` currently only prints a greeting. To record, the daemon must first observe the keyboard: the user holds a capture hotkey to start recording and releases it to stop.

Global key interception on Linux is not a single well-supported mechanism across desktops. The project must work across major desktop environments (GNOME/GTK, KDE/KWayland, COSMIC, Wayland). The architecture already relies on PipeWire (audio), `/dev/uinput` (input injection), and D-Bus (notifications); a keyboard-capture dependency is a new external dependency the daemon must adopt.

## Goals / Non-Goals

**Goals:**
- Detect the <kbd>Ctrl</kbd>+<kbd>Super</kbd>+<kbd>Space</kbd> capture key as a press (inactive→active) and release (active→inactive).
- Log a distinct entry on press and on release so an operator can verify interception from the daemon's log output.
- Detect press even when one modifier is already held before the other, and detect release as soon as any key is dropped.

**Non-Goals:**
- Starting or stopping audio recording (PipeWire).
- Triggering transcription (Parakeet) or injecting text (`/dev/uinput`).
- Making the capture key configurable.
- Broadcasting D-Bus notifications for press/release (a later capability).

## Decisions

### Decision: Register the capture key as a base key with modifiers, not a modifier-only combination

The capture key is <kbd>Ctrl</kbd>+<kbd>Super</kbd>+<kbd>Space</kbd>. A `kbd` hotkey requires a non-modifier base key, so <kbd>Space</kbd> serves as the base key and <kbd>Ctrl</kbd> and <kbd>Super</kbd> are the modifiers. The callback fires when <kbd>Space</kbd> is pressed while both <kbd>Ctrl</kbd> and <kbd>Super</kbd> are held; release is detected by polling the held modifier state.

Rationale: A modifier-only combination has no base key for `kbd-global` to bind to, so it never triggers a hotkey callback. Adding <kbd>Space</kbd> as the base key makes the combination a real hotkey.

### Decision: Use a global key-capture dependency (candidate: `evdev`/`uinput`-adjacent or `wayland`/`udev` global grab)

Global key interception must work across desktops. The design adopts a single global keyboard-event source for the daemon rather than per-application hooks, because the capture key must fire regardless of which window is focused. The exact binding is chosen at implementation time to satisfy "works on most widely used desktop environments and the terminal." A reasonable starting point is a crate that registers a global hotkey/keyboard listener (e.g. `evdev` grabbing the keyboard, or a Wayland global key source), with the concrete choice validated during implementation.

Rationale: The capture must be global, so a per-window or accessibility-API approach is unsuitable. A single source keeps the state machine above in one place. The choice is left open in the spec (behavior only) and pinned here as an implementation decision with a validation step.

### Decision: Log via the daemon's standard logging channel

Press and release are logged through a standard Rust logging mechanism so the output is greppable and distinguishable (e.g. a `capture_key` module with `pressed` / `released` markers). The log content is the externally observable signal the spec's logging requirement checks.

### Decision: Isolate the key source behind a trait for testing

Expose the keyboard event source behind a small trait so the state machine (active derivation, transition detection, logging) can be tested with a synthetic event stream, independent of the real OS capture binding.

## Risks / Trade-offs

- **Desktop/Wayland compatibility**: A global key grab may require elevated permissions or may not behave identically across Wayland sessions. Mitigation: validate against the target desktops (GNOME/GTK, KDE/KWayland, COSMIC) during implementation; if a single binding cannot cover all, prefer a source that works on the most common and document the limitation.
- **Hotkey conflict with the desktop environment**: <kbd>Ctrl</kbd>+<kbd>Super</kbd>+<kbd>Space</kbd> may be claimed by a desktop environment for its own action, so the daemon's grab could be starved. Mitigation: the spec fixes the combination; conflict handling (configurability, override) is explicitly a non-goal here and deferred.
- **Stuck-state edge cases**: If a modifier release event is missed, the daemon could think the key is still held. Mitigation: treat each raw modifier state update as authoritative and recompute `active`; do not rely on paired press/release bookkeeping.
- **Permission requirement**: A global keyboard grab may need the user to be in an input group or run with elevated rights. This is surfaced in tasks as a validation step rather than hidden.

## Validation status

The capture-key state model, transition detection, logging, and the
`KeyEventSource` abstraction are implemented and verified with unit tests
(`cargo test`, 8 passing). `cargo build` and `cargo clippy` pass.

The physical verification of the real global grab (tasks 4.2/4.3/5.2) could
not be completed in the implementation environment because:

- The environment is a headless Wayland session (`XDG_SESSION_TYPE=wayland`,
  no X session) with no physical keyboard to press.
- The user is not in the `input` group (gid 995) and `sudo` requires a
  password, so `/dev/input/eventX` is unreadable (`Permission denied`).

Running `cargo run -p steno-daemon` therefore exits 1 with the expected
message:

```
capture_key: failed to open keyboard source: no evdev keyboard device
supporting the capture keycodes
```

This is the predicted permission risk above, realized. To complete the physical verification: add the user to the `input` group (`sudo usermod -aG input $USER`), re-login, run the daemon, and hold/release <kbd>Ctrl</kbd>+<kbd>Super</kbd>+<kbd>Space</kbd>, confirming a `capture_key: pressed` and a `capture_key: released` line in the log output.

## Bug found during physical verification and fix

Physical testing revealed the real binding captured no events even though it started successfully. The original `EvdevKeyEventSource` opened a **single** device — the first in `evdev::enumerate()` that supported any capture keycode — and exclusively grabbed it. On a keyboard split across multiple evdev nodes (e.g. `Keychron Q3 … Keyboard`, `… Mouse`, `… System Control`, `… Consumer Control`), the grabbed node was not the one the modifier was delivered to, so the daemon read nothing.

Fix (in `crates/steno-daemon/src/capture_key.rs`): open **every** evdev device that supports the capture keycodes, read from **all** of them, and merge modifier state across them, **without** an exclusive grab. This captures the combination regardless of which node the kernel reports it on, and keeps the desktop environment receiving events.

Verification: `cargo build`, `cargo test` (8 passing), and `cargo clippy` pass. The state machine, transition detection, and logging were unchanged (they are unit-tested correct); only the real binding was changed. The physical <kbd>Ctrl</kbd>+<kbd>Super</kbd>+<kbd>Space</kbd> press/release check still requires a desktop session with `input`-group access (run the daemon, hold/release the combination, expect a `capture_key: pressed` then `capture_key: released` line).

## Migration to kbd-global (supersedes the evdev grab)

The evdev grab approaches (grab-one, then grab-all) did not work on the
Cosmic (Wayland) session and, in the grab-all form, exclusively captured
**every** input node, which broke the user's session. The daemon was
migrated to the `kbd-global` crate (`kbd-global = "0.2.0"`, `kbd = "0.2.0"`),
which owns evdev device discovery, hotplug, and the event loop.

The capture key is registered as a global hotkey (`"Ctrl+Super+Space"`), so `kbd-global` detects the combined press and fires a callback. The callback logs `capture_key: pressed` and sets an `active` flag; the release is detected by polling `manager.active_modifiers()` and logging `capture_key: released` when the held modifiers no longer contain both `Ctrl` and `Super`. The manager is built **without grab mode**, so it forwards unmatched events to the desktop rather than exclusively capturing the keyboard — the desktop stays usable.

`crates/steno-daemon/src/capture_key.rs` was rewritten around `HotkeyManager`; `main.rs` builds the capture hotkey and calls `CaptureKey::run`. `cargo build`, `cargo clippy`, and `cargo test` pass; the daemon starts and runs without the grab-all failure.

Caveat to verify physically: `kbd-global` fires the callback once per activation (not on release), so the release is derived from the modifier state. Confirm on the Cosmic session that both `capture_key: pressed` and `capture_key: released` are logged for a physical hold/release of <kbd>Ctrl</kbd>+<kbd>Super</kbd>+<kbd>Space</kbd>.
