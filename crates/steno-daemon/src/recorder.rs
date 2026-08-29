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
        while let Some(msg) = Self::next_command(&mut rx, &ct).await {
            self.handle_command(msg).await;
        }
        self.dispose();
    }

    /// The next command, or None once cancellation is requested.
    async fn next_command(
        rx: &mut Receiver<RecorderCommand>,
        ct: &CancellationToken,
    ) -> Option<RecorderCommand> {
        tokio::select! {
            _ = ct.cancelled() => None,
            msg = rx.recv() => msg,
        }
    }

    /// Best-effort cleanup so shutdown never leaves the microphone open.
    fn dispose(mut self) {
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

        if let Err(reason) = Self::await_ready(ready).await {
            let _ = session.stop();
            tracing::error!("capture failed to start: {reason}");
            return;
        }

        self.session = Some(session);
        self.begin_capture();
    }

    /// Resolve the capture readiness channel within [`START_TIMEOUT`],
    /// flattening every failure mode into one reason string.
    async fn await_ready(
        ready: tokio::sync::oneshot::Receiver<Result<(), String>>,
    ) -> Result<(), String> {
        match tokio::time::timeout(START_TIMEOUT, ready).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => Err("capture readiness task ended".to_owned()),
            Err(_) => Err(format!("capture failed to start within {START_TIMEOUT:?}")),
        }
    }

    /// Announce the recording, attaching a debug WAV path when enabled.
    fn begin_capture(&mut self) {
        if !self.debug {
            tracing::info!("recording started");
            return;
        }
        let path = PathBuf::from(WAV_DIR).join(format!(
            "{}.wav",
            chrono::Local::now().format("%Y%m%d-%H%M%S-%3f")
        ));
        tracing::info!("recording started, saving to {}", path.display());
        self.wav_path = Some(path);
    }

    async fn stop(&mut self) {
        if !self.is_recording() {
            tracing::debug!("not recording, ignoring Stop");
            return;
        }

        let Some(session) = self.session.take() else {
            return;
        };
        let samples = match session.stop() {
            Ok(samples) => samples,
            Err(e) => {
                tracing::error!("capture ended with error: {e}");
                return;
            }
        };

        self.finish_capture(samples).await;
    }

    /// Process the finished capture: save (in debug mode) and transcribe.
    async fn finish_capture(&mut self, samples: Vec<f32>) {
        if samples.is_empty() {
            tracing::warn!("capture produced no audio");
            return;
        }

        if !self.save_debug_wav(&samples) {
            return;
        }

        self.transcribe(samples);
    }

    /// Write the debug WAV when one was requested. Returns false if the
    /// capture should be discarded because the write failed.
    fn save_debug_wav(&mut self, samples: &[f32]) -> bool {
        let Some(path) = self.wav_path.take() else {
            return true;
        };
        if let Err(e) = wav::write_wav(&path, samples) {
            tracing::error!("failed to write {}: {e}", path.display());
            return false;
        }
        tracing::info!(
            "saved recording to {} ({} s)",
            path.display(),
            samples.len() as f64 / wav::SAMPLE_RATE as f64
        );
        true
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
