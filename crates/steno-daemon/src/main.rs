use anyhow::Result;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use steno_daemon::listener::KeyListener;
use steno_daemon::notifications::{self, DictationEvent};
use steno_daemon::recorder::{Recorder, RecorderCommand};
use steno_daemon::uinput::{Injector, UinputDevice};
use tokio::sync::mpsc::channel;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();

    let debug = std::env::args().any(|arg| arg == "--debug");
    if debug {
        tracing::info!("debug mode enabled, recordings are saved to /tmp/steno");
    }

    let token = CancellationToken::new();
    let tracker = TaskTracker::new();

    // Create a command channel between the key listener and the recorder so we
    // can start/stop the recording depending on the key state.
    let (tx, rx) = channel::<RecorderCommand>(32);

    // Ensure the transcription model is available.
    steno_daemon::model::ensure_parakeet_model().await?;

    // Dictation lifecycle notifications ride the session bus through a
    // dedicated task. An unreachable bus downgrades to discard mode inside
    // `connect` — it never stops the daemon (unlike /dev/uinput below).
    let (notifier_rx_tx, notifier_rx) = channel::<DictationEvent>(64);
    let (notifier, resource) = notifications::Notifier::connect().await;
    if let Some(resource) = resource {
        tracker.spawn(notifications::serve_resource(resource, token.clone()));
    }
    tracker.spawn(notifier.listen(notifier_rx, token.clone()));

    // Create the virtual keyboard before anything else: the daemon must
    // not run in a state where captured text has nowhere to go.
    let injector = Injector::with_notifier(UinputDevice::open()?, notifier_rx_tx.clone());

    // Text to inject flows from the recorder (transcription output) to
    // the injector task through this channel, FIFO.
    let (inject_tx, inject_rx) = channel::<String>(16);

    // Load the Parakeet TDT model. With the `cuda` feature the CUDA
    // execution provider is requested; it still falls back to CPU at
    // session-build time when CUDA is unusable.
    let model_dir = steno_daemon::model::parakeet_model_dir()?;
    let model_config = parakeet_rs::ExecutionConfig::new();
    #[cfg(feature = "cuda")]
    let model_config = model_config.with_execution_provider(parakeet_rs::ExecutionProvider::Cuda);
    // The blocking GPU/CPU session load runs before any task exists, so it
    // never starves the runtime.
    let model = Arc::new(Mutex::new(parakeet_rs::ParakeetTDT::from_pretrained(
        &model_dir,
        Some(model_config),
    )?));
    let recorder = Recorder::new(model, tracker.clone(), debug, inject_tx, notifier_rx_tx);

    // Constructed only now, after the potentially multi-minute first-run
    // model download and load: KeyListener::new exclusively grabs evdev
    // keyboards, so Ctrl+Super must not be swallowed while the daemon
    // still cannot capture anything.
    let listener = KeyListener::new()?;

    // Spawn the key listener that will grab Ctrl+Super to trigger the recording of audio.
    // It will poll every 15 milliseconds for the recording trigger.
    let listener_task = tracker.spawn(listener.listen(tx, token.clone()));
    let recorder_task = tracker.spawn(recorder.listen(rx, token.clone()));
    let injector_task = tracker.spawn(injector.listen(inject_rx, token.clone()));
    tracker.close();

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    tracing::info!("daemon active");

    // A dying capture path is a daemon failure: surface it and exit non-zero
    // instead of waiting for a shutdown signal with dead capture.
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("received SIGINT");
        }
        _ = sigterm.recv() => {
            tracing::info!("received SIGTERM");
        }
        result = listener_task => {
            return fail_task(
                "key listener",
                match result {
                    Ok(Err(err)) => err,
                    Ok(Ok(())) => anyhow::anyhow!("task exited unexpectedly"),
                    Err(join) => anyhow::anyhow!("task ended: {join:?}"),
                },
            );
        }
        result = recorder_task => {
            return fail_task(
                "recorder",
                match result {
                    Ok(()) => anyhow::anyhow!("task exited unexpectedly"),
                    Err(join) => anyhow::anyhow!("task ended: {join:?}"),
                },
            );
        }
        result = injector_task => {
            return fail_task(
                "injector",
                match result {
                    Ok(()) => anyhow::anyhow!("task exited unexpectedly"),
                    Err(join) => anyhow::anyhow!("task ended: {join:?}"),
                },
            );
        }
    }

    token.cancel();

    let timed_out = timeout(Duration::from_secs(30), tracker.wait())
        .await
        .is_err();

    if timed_out {
        tracing::warn!("shutdown timed out, forcing exit");
    }

    Ok(())
}

fn fail_task(task: &'static str, err: anyhow::Error) -> Result<()> {
    tracing::error!("{task} task failed: {err:#}");
    Err(err)
}
