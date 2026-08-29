//! End-to-end capture tests: drive the recorder via commands and verify the
//! WAV artifact behavior. In debug mode a WAV is written to a private temp
//! directory and transcribed; without debug mode no WAV is written.
//!
//! Requires a running PipeWire session (with a microphone) and the
//! provisioned parakeet model. The recorder writes to a per-process temp
//! dir, so a concurrently running daemon using /tmp/steno cannot interfere.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use hound::SampleFormat;
#[cfg(feature = "cuda")]
use parakeet_rs::ExecutionProvider;
use parakeet_rs::{ExecutionConfig, ParakeetTDT, Transcriber};
use steno_daemon::recorder::{Recorder, RecorderCommand};
use tokio::sync::mpsc::channel;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

// The tests share the microphone; cargo runs them in parallel threads, so
// serialize the capture flows. (The WAV dir is private per process, but a
// second `cargo test -- --ignored` run would still contend for the mic.)
static CAPTURE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Private debug-WAV directory for this test process — never /tmp/steno, so
/// a live daemon's recordings cannot be mistaken for the test's.
fn wav_dir() -> PathBuf {
    std::env::temp_dir().join(format!("steno-capture-test-{}", std::process::id()))
}

fn existing_wavs(dir: &std::path::Path) -> std::io::Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "wav") {
            paths.push(path);
        }
    }
    Ok(paths)
}

fn cuda_model() -> Result<ParakeetTDT> {
    let model_dir = steno_daemon::model::parakeet_model_dir()?;
    let config = ExecutionConfig::new();
    #[cfg(feature = "cuda")]
    let config = config.with_execution_provider(ExecutionProvider::Cuda);
    Ok(ParakeetTDT::from_pretrained(&model_dir, Some(config))?)
}

/// Drive a Start → record → Stop cycle and let the recorder task drain.
async fn run_capture_flow(model: Arc<Mutex<ParakeetTDT>>, debug: bool) -> Result<()> {
    let (tx, rx) = channel::<RecorderCommand>(32);
    // The injection channel's receiver is held open so transcription
    // delivery succeeds; the test asserts on WAVs, not injected text.
    let (inject_tx, _inject_rx) = channel::<String>(16);
    let recorder = Recorder::with_wav_dir(model, TaskTracker::new(), debug, wav_dir(), inject_tx);
    let token = CancellationToken::new();
    let recorder_task = tokio::spawn(recorder.listen(rx, token.clone()));

    record_two_seconds(&tx).await?;
    shutdown_recorder(token, tx, recorder_task).await
}

/// Start, record for two seconds, then Stop.
async fn record_two_seconds(tx: &tokio::sync::mpsc::Sender<RecorderCommand>) -> Result<()> {
    tx.send(RecorderCommand::Start).await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    tx.send(RecorderCommand::Stop).await?;
    // The WAV write happens inside the Stop handling; give the command a
    // moment to be processed before the caller inspects the directory.
    tokio::time::sleep(Duration::from_secs(2)).await;
    Ok(())
}

async fn shutdown_recorder(
    token: CancellationToken,
    tx: tokio::sync::mpsc::Sender<RecorderCommand>,
    recorder_task: tokio::task::JoinHandle<()>,
) -> Result<()> {
    token.cancel();
    drop(tx);
    recorder_task.await?;
    Ok(())
}

fn new_wav_since(dir: &std::path::Path, before: &[PathBuf]) -> std::io::Result<Option<PathBuf>> {
    Ok(existing_wavs(dir)?
        .into_iter()
        .find(|path| !before.contains(path)))
}

#[tokio::test]
#[ignore = "requires a running PipeWire session and the provisioned parakeet model"]
async fn it_writes_wav_and_transcribes_in_debug_mode() -> Result<()> {
    let _ = tracing_subscriber::fmt().try_init();

    let _guard = CAPTURE_LOCK.lock().await;

    let dir = wav_dir();
    let _ = std::fs::remove_dir_all(&dir);
    let before = existing_wavs(&dir)?;
    steno_daemon::model::ensure_parakeet_model().await?;

    let model = Arc::new(Mutex::new(cuda_model()?));
    run_capture_flow(Arc::clone(&model), true).await?;

    let new_wav = new_wav_since(&dir, &before)?.expect("no new WAV appeared in debug mode");

    let reader = hound::WavReader::open(&new_wav)?;
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, 16_000);
    assert_eq!(spec.channels, 1);
    assert_eq!(spec.bits_per_sample, 32);
    assert_eq!(spec.sample_format, SampleFormat::Float);

    let transcription = tokio::task::spawn_blocking(move || {
        model
            .lock()
            .expect("model mutex poisoned")
            .transcribe_file(&new_wav, None)
    })
    .await??;
    tracing::info!("transcription of captured audio: {}", transcription.text);

    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[tokio::test]
#[ignore = "requires a running PipeWire session and the provisioned parakeet model"]
async fn it_skips_wav_without_debug() -> Result<()> {
    let _ = tracing_subscriber::fmt().try_init();

    let _guard = CAPTURE_LOCK.lock().await;

    let dir = wav_dir();
    let _ = std::fs::remove_dir_all(&dir);
    let before = existing_wavs(&dir)?;
    steno_daemon::model::ensure_parakeet_model().await?;

    let model = Arc::new(Mutex::new(cuda_model()?));
    run_capture_flow(model, false).await?;

    let new_wav = new_wav_since(&dir, &before)?;
    assert!(
        new_wav.is_none(),
        "unexpected WAV written without debug mode: {new_wav:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
