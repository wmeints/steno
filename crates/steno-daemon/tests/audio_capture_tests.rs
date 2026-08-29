//! End-to-end capture tests: drive the recorder via commands and verify the
//! WAV artifact behavior. In debug mode a WAV is written to `/tmp/steno` and
//! transcribed; without `--debug` no WAV is written.
//!
//! Requires a running PipeWire session (with a microphone) and the
//! provisioned parakeet model.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use hound::SampleFormat;
use parakeet_rs::{ExecutionConfig, ExecutionProvider, ParakeetTDT, Transcriber};
use steno_daemon::recorder::{Recorder, RecorderCommand};
use tokio::sync::mpsc::channel;
use tokio_util::sync::CancellationToken;

const WAV_DIR: &str = "/tmp/steno";

// The tests share the microphone and /tmp/steno; cargo runs them in parallel
// threads, so serialize the capture flows.
static CAPTURE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn existing_wavs() -> std::io::Result<Vec<PathBuf>> {
    let dir = std::path::Path::new(WAV_DIR);
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
    Ok(ParakeetTDT::from_pretrained(
        &model_dir,
        Some(ExecutionConfig::new().with_execution_provider(ExecutionProvider::Cuda)),
    )?)
}

/// Drive a Start → record → Stop cycle and let the recorder task drain.
async fn run_capture_flow(model: Arc<Mutex<ParakeetTDT>>, debug: bool) -> Result<()> {
    let recorder = Recorder::new(model, debug);
    let (tx, rx) = channel::<RecorderCommand>(32);
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
    // The WAV write happens synchronously inside stop(); give the command a
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

fn new_wav_since(before: &[PathBuf]) -> std::io::Result<Option<PathBuf>> {
    Ok(existing_wavs()?
        .into_iter()
        .find(|path| !before.contains(path)))
}

#[tokio::test]
#[ignore = "requires a running PipeWire session and the provisioned parakeet model"]
async fn it_writes_wav_and_transcribes_in_debug_mode() -> Result<()> {
    let _ = tracing_subscriber::fmt().try_init();

    let _guard = CAPTURE_LOCK.lock().await;

    let before = existing_wavs()?;
    steno_daemon::model::ensure_parakeet_model().await?;

    let model = Arc::new(Mutex::new(cuda_model()?));
    run_capture_flow(Arc::clone(&model), true).await?;

    let new_wav = new_wav_since(&before)?.expect("no new WAV appeared in /tmp/steno in debug mode");

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

    Ok(())
}

#[tokio::test]
#[ignore = "requires a running PipeWire session and the provisioned parakeet model"]
async fn it_skips_wav_without_debug() -> Result<()> {
    let _ = tracing_subscriber::fmt().try_init();

    let _guard = CAPTURE_LOCK.lock().await;

    let before = existing_wavs()?;
    steno_daemon::model::ensure_parakeet_model().await?;

    let model = Arc::new(Mutex::new(cuda_model()?));
    run_capture_flow(model, false).await?;

    let new_wav = new_wav_since(&before)?;
    assert!(
        new_wav.is_none(),
        "unexpected WAV written without debug mode: {new_wav:?}"
    );

    Ok(())
}
