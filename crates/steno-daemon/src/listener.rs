use crate::recorder::RecorderCommand;
use anyhow::Result;
use kbd::hotkey::Modifier;
use kbd_global::backend::Backend;
use kbd_global::manager::HotkeyManager;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

/// Pure state machine for the <kbd>Ctrl</kbd>+<kbd>Super</kbd> capture key.
///
/// OS-free: it is fed the currently held modifier bits and derives the
/// active state and transitions, so the capture logic is unit-testable
/// without any evdev keyboard. The capture key is the pair of modifiers
/// alone — there is no base key.
#[derive(Debug, Default)]
pub struct CaptureState {
    active: bool,
}

impl CaptureState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed the currently held modifier state and return the command the
    /// transition, if any, requires.
    pub fn handle(&mut self, ctrl: bool, super_: bool) -> Option<RecorderCommand> {
        let active = ctrl && super_;

        if active && !self.active {
            self.active = true;
            Some(RecorderCommand::Start)
        } else if !active && self.active {
            self.active = false;
            Some(RecorderCommand::Stop)
        } else {
            None
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}

pub struct KeyListener {
    hotkey_mgr: HotkeyManager,
    state: CaptureState,
}

impl KeyListener {
    pub fn new() -> Result<KeyListener> {
        let hotkey_mgr = HotkeyManager::builder()
            .backend(Backend::Evdev)
            .grab()
            .build()?;

        Ok(Self {
            hotkey_mgr,
            state: CaptureState::new(),
        })
    }

    pub async fn listen(
        mut self,
        tx: Sender<RecorderCommand>,
        token: CancellationToken,
    ) -> Result<()> {
        let mut ticker = interval(Duration::from_millis(15));

        loop {
            let modifiers = self.hotkey_mgr.active_modifiers()?;

            // Forward only transitions; an idle tick is a no-op.
            if let Some(cmd) = self.state.handle(
                modifiers.contains(Modifier::Ctrl),
                modifiers.contains(Modifier::Super),
            ) {
                match cmd {
                    RecorderCommand::Start => tracing::info!("capture key pressed"),
                    RecorderCommand::Stop => tracing::info!("capture key released"),
                }
                tx.send(cmd).await?;
            }

            tokio::select! {
                _ = token.cancelled() => break,
                _ = ticker.tick() => {}
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn press_from_idle_emits_start() {
        let mut subject = CaptureState::new();

        assert_eq!(subject.handle(true, true), Some(RecorderCommand::Start));
        assert!(subject.is_active());
    }

    #[test]
    fn press_while_modifier_already_held_emits_start() {
        let mut subject = CaptureState::new();

        // One modifier held first is not a press.
        assert_eq!(subject.handle(true, false), None);
        assert!(!subject.is_active());

        // Adding the other modifier is the press.
        assert_eq!(subject.handle(true, true), Some(RecorderCommand::Start));
        assert!(subject.is_active());
    }

    #[test]
    fn single_modifier_does_not_activate() {
        let mut subject = CaptureState::new();
        assert_eq!(subject.handle(true, false), None);
        assert!(!subject.is_active());

        let mut subject = CaptureState::new();
        assert_eq!(subject.handle(false, true), None);
        assert!(!subject.is_active());
    }

    #[test]
    fn holding_both_continuously_emits_nothing() {
        let mut subject = CaptureState::new();
        subject.handle(true, true);

        assert_eq!(subject.handle(true, true), None);
        assert!(subject.is_active());
    }

    #[test]
    fn release_on_either_modifier_dropped_emits_stop() {
        let mut subject = CaptureState::new();
        subject.handle(true, true);

        assert_eq!(subject.handle(true, false), Some(RecorderCommand::Stop));
        assert!(!subject.is_active());

        let mut subject = CaptureState::new();
        subject.handle(true, true);

        assert_eq!(subject.handle(false, true), Some(RecorderCommand::Stop));
        assert!(!subject.is_active());
    }

    #[test]
    fn release_both_together_emits_stop() {
        let mut subject = CaptureState::new();
        subject.handle(true, true);

        assert_eq!(subject.handle(false, false), Some(RecorderCommand::Stop));
        assert!(!subject.is_active());
    }

    #[test]
    fn repeated_cycles_emit_transitions_in_order() {
        let mut subject = CaptureState::new();
        let mut seen = Vec::new();

        for (ctrl, super_) in [(true, true), (false, false), (true, true), (false, false)] {
            if let Some(cmd) = subject.handle(ctrl, super_) {
                seen.push(cmd);
            }
        }

        assert_eq!(
            seen,
            [
                RecorderCommand::Start,
                RecorderCommand::Stop,
                RecorderCommand::Start,
                RecorderCommand::Stop
            ]
        );
        assert!(!subject.is_active());
    }

    #[test]
    fn idle_ticks_emit_nothing() {
        let mut subject = CaptureState::new();

        assert_eq!(subject.handle(false, false), None);
        assert_eq!(subject.handle(true, false), None);
        // Still a single modifier on the next tick: no command, still inactive.
        assert_eq!(subject.handle(true, false), None);
        assert!(!subject.is_active());
    }

    #[test]
    fn ticks_after_release_emit_nothing() {
        let mut subject = CaptureState::new();
        subject.handle(true, true);
        subject.handle(false, false);

        assert_eq!(subject.handle(false, false), None);
        assert!(!subject.is_active());
    }
}
