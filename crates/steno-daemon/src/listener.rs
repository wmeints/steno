use crate::recorder::RecorderCommand;
use anyhow::Result;
use kbd::hotkey::{Modifier, ModifierSet};
use kbd_global::backend::Backend;
use kbd_global::manager::HotkeyManager;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

pub struct KeyListener {
    hotkey_mgr: HotkeyManager,
    is_active: bool,
}

impl KeyListener {
    pub fn new() -> Result<KeyListener> {
        let hotkey_mgr = HotkeyManager::builder()
            .backend(Backend::Evdev)
            .grab()
            .build()?;

        Ok(Self {
            hotkey_mgr,
            is_active: false,
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

            self.handle_modifiers(modifiers)?
                .map(|cmd| match cmd {
                    RecorderCommand::Start => tx.send(cmd),
                    RecorderCommand::Stop => tx.send(cmd),
                })
                .unwrap()
                .await?;

            tokio::select! {
                _ = token.cancelled() => break,
                _ = ticker.tick() => {}
            }
        }

        Ok(())
    }

    fn handle_modifiers(&mut self, modifiers: ModifierSet) -> Result<Option<RecorderCommand>> {
        if modifiers.contains(Modifier::Ctrl) && modifiers.contains(Modifier::Super) {
            // Activate the recording mode, and send the start command to the recorder.
            // The recorder will start capturing audio.
            if !self.is_active {
                self.is_active = true;
                return Ok(Some(RecorderCommand::Start));
            }
        } else {
            // Deactivate the recording mode, and send the stop command to the recorder.
            // The recorder will handle the transcription after this.
            if self.is_active {
                self.is_active = false;
                return Ok(Some(RecorderCommand::Stop));
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_modifiers_inactive_start_command() -> Result<()> {
        let mut subject = KeyListener::new().expect("new key listener");
        let modifiers = ModifierSet::NONE.with(Modifier::Ctrl).with(Modifier::Super);

        subject
            .handle_modifiers(modifiers)
            .map(|cmd| match cmd {
                Some(RecorderCommand::Start) => Ok(()),
                Some(RecorderCommand::Stop) => anyhow::bail!("invalid command"),
                None => Ok(()),
            })
            .unwrap()
    }

    #[test]
    fn test_handle_modifiers_active_stop_command() -> Result<()> {
        let mut subject = KeyListener::new().expect("new key listener");
        let active_mods = ModifierSet::NONE.with(Modifier::Ctrl).with(Modifier::Super);
        let inactive_mods = ModifierSet::NONE;

        subject.handle_modifiers(active_mods)?;

        subject
            .handle_modifiers(inactive_mods)
            .map(|cmd| match cmd {
                Some(RecorderCommand::Start) => anyhow::bail!("unexpected start"),
                Some(RecorderCommand::Stop) => Ok(()),
                None => Ok(()),
            })
            .unwrap()
    }

    #[test]
    fn test_handle_modifiers_inactive_no_commands() -> Result<()> {
        let mut subject = KeyListener::new().expect("new key listener");
        let active_mods = ModifierSet::NONE.with(Modifier::Ctrl).with(Modifier::Super);
        let inactive_mods = ModifierSet::NONE;

        // Activate and then deactivate the recording mode.
        subject.handle_modifiers(active_mods)?;
        subject.handle_modifiers(inactive_mods)?;

        // This should not produce any commands
        subject
            .handle_modifiers(inactive_mods)
            .map(|cmd| match cmd {
                Some(RecorderCommand::Start) => anyhow::bail!("unexpected start"),
                Some(RecorderCommand::Stop) => anyhow::bail!("unexpected stop"),
                None => Ok(()),
            })
            .unwrap()
    }
}
