use tokio::sync::mpsc::Receiver;
use tokio_util::sync::CancellationToken;

pub enum RecorderCommand {
    Start,
    Stop,
}

pub struct Recorder {
    is_recording: bool,
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            is_recording: false,
        }
    }

    pub async fn listen(mut self, mut rx: Receiver<RecorderCommand>, ct: CancellationToken) {
        loop {
            tokio::select! {
                _ = ct.cancelled() => break,
                Some(msg) = rx.recv() => self.handle_command(msg),
            }
        }
    }

    pub fn is_recording(&self) -> bool {
        self.is_recording
    }

    fn handle_command(&mut self, msg: RecorderCommand) {
        if let RecorderCommand::Start = msg {
            self.is_recording = true;
            tracing::info!("Start recording");
        }

        if let RecorderCommand::Stop = msg
            && self.is_recording
        {
            self.is_recording = false;
            tracing::info!("Stop recording");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_start_without_recording() {
        let mut subject = Recorder::new();

        assert_eq!(subject.is_recording, false);

        subject.handle_command(RecorderCommand::Start);

        assert!(subject.is_recording);
    }

    #[test]
    fn test_stop_while_recording() {
        let mut subject = Recorder::new();

        assert_eq!(subject.is_recording, false);

        subject.handle_command(RecorderCommand::Start);
        subject.handle_command(RecorderCommand::Stop);

        assert!(!subject.is_recording);
    }
}
