//! Recorder lifecycle: capture microphone audio on Start, write it to a
//! timestamped WAV when debug mode is enabled, trigger Parakeet TDT
//! transcription of the recording, and hand finished transcriptions to the
//! injector task. Recording and transcription starts are announced as
//! dictation events on the notification channel.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use parakeet_rs::Transcriber;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::notifications::{self, DictationEvent};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

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

pub struct Recorder<M = parakeet_rs::ParakeetTDT> {
    model: Arc<Mutex<M>>,
    tracker: TaskTracker,
    debug: bool,
    wav_dir: PathBuf,
    session: Option<CaptureSession>,
    wav_path: Option<PathBuf>,
    inject_tx: Sender<String>,
    notifier_tx: Sender<DictationEvent>,
}

impl<M: Transcriber + Send + 'static> Recorder<M> {
    pub fn new(
        model: Arc<Mutex<M>>,
        tracker: TaskTracker,
        debug: bool,
        inject_tx: Sender<String>,
        notifier_tx: Sender<DictationEvent>,
    ) -> Self {
        Self::with_wav_dir(
            model,
            tracker,
            debug,
            PathBuf::from(WAV_DIR),
            inject_tx,
            notifier_tx,
        )
    }

    /// Build a recorder writing debug WAVs to `wav_dir` (tests inject a
    /// private directory so a concurrently running daemon cannot interfere).
    pub fn with_wav_dir(
        model: Arc<Mutex<M>>,
        tracker: TaskTracker,
        debug: bool,
        wav_dir: impl Into<PathBuf>,
        inject_tx: Sender<String>,
        notifier_tx: Sender<DictationEvent>,
    ) -> Self {
        Self {
            model,
            tracker,
            debug,
            wav_dir: wav_dir.into(),
            session: None,
            wav_path: None,
            inject_tx,
            notifier_tx,
        }
    }

    pub async fn listen(mut self, mut rx: Receiver<RecorderCommand>, ct: CancellationToken) {
        while let Some(msg) = Self::next_command(&mut rx, &ct).await {
            self.handle_command(msg).await;
        }
        self.dispose().await;
    }

    /// The next command, or None once cancellation is requested and no
    /// command is already queued. Biased so a Stop delivered just before
    /// cancellation still flushes its recording before shutdown.
    async fn next_command(
        rx: &mut Receiver<RecorderCommand>,
        ct: &CancellationToken,
    ) -> Option<RecorderCommand> {
        tokio::select! {
            biased;
            msg = rx.recv() => msg,
            () = ct.cancelled(), if rx.is_empty() => None,
            else => None,
        }
    }

    /// Best-effort cleanup so shutdown never leaves the microphone open.
    async fn dispose(&mut self) {
        if let Some(session) = self.session.take() {
            // Joining the capture thread blocks; keep the runtime responsive.
            let _ = tokio::task::spawn_blocking(move || session.stop()).await;
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
        notifications::emit(&self.notifier_tx, DictationEvent::RecordingStarted);
        if !self.debug {
            tracing::info!("recording started");
            return;
        }
        let path = self.wav_dir.join(format!(
            "{}.wav",
            chrono::Local::now().format("%Y%m%d-%H%M%S-%3f")
        ));
        tracing::info!("recording started, saving to {}", path.display());
        self.wav_path = Some(path);
    }

    async fn stop(&mut self) {
        let Some(session) = self.session.take() else {
            tracing::debug!("not recording, ignoring Stop");
            return;
        };

        // Joining the capture thread blocks; keep the runtime responsive.
        let (samples, error) = tokio::task::spawn_blocking(move || session.stop())
            .await
            .expect("capture drain task panicked");

        self.finish_capture(samples, error).await;
    }

    /// Report a capture error, then save (in debug mode) and transcribe.
    /// Samples captured before a mid-session error are salvaged.
    async fn finish_capture(&mut self, samples: Vec<f32>, error: Option<String>) {
        report_capture_error(error);
        if samples.is_empty() {
            tracing::warn!("capture produced no audio");
            return;
        }

        self.save_debug_wav(&samples).await;
        self.transcribe(samples);
    }

    /// Write the debug WAV when one was requested. A failed write is logged
    /// only; transcription of the in-memory capture still proceeds.
    async fn save_debug_wav(&mut self, samples: &[f32]) {
        let Some(path) = self.wav_path.take() else {
            return;
        };
        let owned = samples.to_vec();
        let logged = path.clone();
        let result = tokio::task::spawn_blocking(move || wav::write_wav(&path, &owned)).await;
        log_wav_result(&logged, samples, result);
    }
    /// Transcribe the captured samples fire-and-forget so a new press during
    /// inference is still served; the model mutex serializes concurrent
    /// inferences. The tracker keeps the daemon's shutdown drain waiting for
    /// in-flight transcriptions. A finished transcription's text is handed
    /// to the injector task.
    fn transcribe(&self, samples: Vec<f32>) {
        notifications::emit(&self.notifier_tx, DictationEvent::TranscriptionStarted);
        let model = Arc::clone(&self.model);
        let inject_tx = self.inject_tx.clone();
        self.tracker.spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                model
                    .lock()
                    .expect("model mutex poisoned")
                    .transcribe_samples(samples, wav::SAMPLE_RATE, 1, None)
            })
            .await;
            match result {
                Ok(Ok(transcription)) => {
                    tracing::info!("transcription: {}", transcription.text);
                    deliver_transcription(&inject_tx, transcription.text).await;
                }
                Ok(Err(e)) => tracing::error!("transcription failed: {e}"),
                Err(join) => tracing::error!("transcription task ended: {join:?}"),
            }
        });
    }
}

/// Send a finished transcription to the injector task, logging delivery
/// failures (a closed or saturated channel must not kill the capture path).
async fn deliver_transcription(inject_tx: &Sender<String>, text: String) {
    if let Err(err) = inject_tx.send(text).await {
        tracing::error!("failed to hand transcription to injector: {err}");
    }
}

/// Surface a capture failure; samples salvaged from before the error are
/// still processed afterwards.
fn report_capture_error(error: Option<String>) {
    if let Some(e) = error {
        tracing::error!("capture ended with error: {e}");
    }
}

/// Log the outcome of a debug WAV write.
fn log_wav_result(
    path: &Path,
    samples: &[f32],
    result: Result<std::io::Result<()>, tokio::task::JoinError>,
) {
    match result {
        Ok(Ok(())) => tracing::info!(
            "saved recording to {} ({} s)",
            path.display(),
            seconds(samples)
        ),
        Ok(Err(e)) => tracing::error!("failed to write {}: {e}", path.display()),
        Err(join) => tracing::error!("wav write task ended: {join:?}"),
    }
}

/// Captured audio length in seconds.
fn seconds(samples: &[f32]) -> f64 {
    samples.len() as f64 / wav::SAMPLE_RATE as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use parakeet_rs::{TimestampMode, TranscriptionResult};
    use tokio::sync::mpsc::channel;

    /// Model that records nothing and answers with fixed text.
    struct FakeModel;

    impl Transcriber for FakeModel {
        fn transcribe_samples(
            &mut self,
            _audio: Vec<f32>,
            _sample_rate: u32,
            _channels: u16,
            _mode: Option<TimestampMode>,
        ) -> parakeet_rs::Result<TranscriptionResult> {
            Ok(TranscriptionResult {
                text: "ok".to_string(),
                tokens: Vec::new(),
            })
        }
    }

    fn recorder_with_events(
        dir: &std::path::Path,
    ) -> (Recorder<FakeModel>, Receiver<DictationEvent>) {
        let model = Arc::new(Mutex::new(FakeModel));
        let (inject_tx, _inject_rx) = channel::<String>(16);
        let (notifier_tx, notifier_rx) = channel::<DictationEvent>(16);
        let recorder = Recorder::with_wav_dir(
            model,
            TaskTracker::new(),
            false,
            dir,
            inject_tx,
            notifier_tx,
        );
        (recorder, notifier_rx)
    }

    #[test]
    fn begin_capture_emits_recording_started() {
        let dir = std::env::temp_dir();
        let (mut recorder, mut events) = recorder_with_events(&dir);
        recorder.begin_capture();
        assert_eq!(events.try_recv().unwrap(), DictationEvent::RecordingStarted);
        assert!(events.try_recv().is_err(), "exactly one event");
    }

    #[tokio::test]
    async fn finish_capture_with_audio_emits_transcription_started() {
        let dir = std::env::temp_dir();
        let (mut recorder, mut events) = recorder_with_events(&dir);
        recorder.finish_capture(vec![0.0; 1600], None).await;
        assert_eq!(
            events.recv().await,
            Some(DictationEvent::TranscriptionStarted)
        );
    }

    #[tokio::test]
    async fn finish_capture_without_audio_emits_nothing() {
        let dir = std::env::temp_dir();
        let (mut recorder, mut events) = recorder_with_events(&dir);
        // Empty samples: capture-error or silent capture, no transcription.
        recorder
            .finish_capture(Vec::new(), Some("boom".to_string()))
            .await;
        recorder.finish_capture(Vec::new(), None).await;
        assert!(events.try_recv().is_err(), "no event without audio");
    }

    #[test]
    fn emit_on_closed_notifier_channel_does_not_panic() {
        let dir = std::env::temp_dir();
        let (mut recorder, events) = recorder_with_events(&dir);
        drop(events);
        recorder.begin_capture(); // must not panic
    }
}
