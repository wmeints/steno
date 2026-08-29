//! Recorder lifecycle: capture microphone audio on Start, write it to a
//! timestamped WAV in `/tmp/steno` on Stop when debug mode is enabled, and
//! trigger Parakeet TDT transcription of the recording.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use parakeet_rs::Transcriber;
use tokio::sync::mpsc::Receiver;
use tokio_util::sync::CancellationToken;

use crate::capture::CaptureSession;
use crate::wav;

/// Directory captured WAVs are written to for debugging.
const WAV_DIR: &str = "/tmp/steno";

/// How long capture may take to reach the streaming state.
const START_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, PartialEq, Eq)]
pub enum RecorderCommand {
    Start,
    Stop,
}

pub struct Recorder {
    model: Arc<Mutex<parakeet_rs::ParakeetTDT>>,
    debug: bool,
    session: Option<CaptureSession>,
    wav_path: Option<PathBuf>,
}

impl Recorder {
    pub fn new(model: Arc<Mutex<parakeet_rs::ParakeetTDT>>, debug: bool) -> Self {
        Self {
            model,
            debug,
            session: None,
            wav_path: None,
        }
    }

    pub async fn listen(mut self, mut rx: Receiver<RecorderCommand>, ct: CancellationToken) {
        loop {
            tokio::select! {
                _ = ct.cancelled() => break,
                Some(msg) = rx.recv() => self.handle_command(msg).await,
            }
        }

        // Best-effort cleanup so shutdown never leaves the microphone open.
        if let Some(session) = self.session.take() {
            let _ = session.stop();
        }
    }

    pub fn is_recording(&self) -> bool {
        self.session.is_some()
    }

    async fn handle_command(&mut self, cmd: RecorderCommand) {
        match cmd {
            RecorderCommand::Start => self.start().await,
            RecorderCommand::Stop => self.stop().await,
        }
    }

    async fn start(&mut self) {
        if self.is_recording() {
            tracing::debug!("already recording, ignoring Start");
            return;
        }

        let (session, ready) = CaptureSession::start();

        match tokio::time::timeout(START_TIMEOUT, ready).await {
            Ok(Ok(Ok(()))) => {}
            Ok(Ok(Err(reason))) => {
                let _ = session.stop();
                tracing::error!("capture failed to start: {reason}");
                return;
            }
            Ok(Err(join)) => {
                let _ = session.stop();
                tracing::error!("capture readiness task ended: {join:?}");
                return;
            }
            Err(_) => {
                let _ = session.stop();
                tracing::error!("capture failed to start within {START_TIMEOUT:?}");
                return;
            }
        }

        self.session = Some(session);
        if self.debug {
            let path = PathBuf::from(WAV_DIR).join(format!(
                "{}.wav",
                chrono::Local::now().format("%Y%m%d-%H%M%S-%3f")
            ));
            tracing::info!("recording started, saving to {}", path.display());
            self.wav_path = Some(path);
        } else {
            tracing::info!("recording started");
        }
    }

    async fn stop(&mut self) {
        if !self.is_recording() {
            tracing::debug!("not recording, ignoring Stop");
            return;
        }

        let session = self
            .session
            .take()
            .expect("is_recording checked the session");

        let samples = match session.stop() {
            Ok(samples) => samples,
            Err(e) => {
                tracing::error!("capture ended with error: {e}");
                return;
            }
        };

        if samples.is_empty() {
            tracing::warn!("capture produced no audio");
            return;
        }

        if let Some(path) = self.wav_path.take() {
            if let Err(e) = wav::write_wav(&path, &samples) {
                tracing::error!("failed to write {}: {e}", path.display());
                return;
            }
            tracing::info!(
                "saved recording to {} ({} s)",
                path.display(),
                samples.len() as f64 / wav::SAMPLE_RATE as f64
            );
        }

        self.transcribe(samples);
    }

    /// Transcribe the captured samples fire-and-forget so a new press during
    /// inference is still served; the model mutex serializes concurrent
    /// inferences.
    fn transcribe(&self, samples: Vec<f32>) {
        let model = Arc::clone(&self.model);
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                model
                    .lock()
                    .expect("model mutex poisoned")
                    .transcribe_samples(samples, wav::SAMPLE_RATE, 1, None)
            })
            .await;
            match result {
                Ok(Ok(transcription)) => tracing::info!("transcription: {}", transcription.text),
                Ok(Err(e)) => tracing::error!("transcription failed: {e}"),
                Err(join) => tracing::error!("transcription task ended: {join:?}"),
            }
        });
    }
}
