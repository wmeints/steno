use anyhow::Result;
use std::time::Duration;
use steno_daemon::listener::KeyListener;
use steno_daemon::recorder::{Recorder, RecorderCommand};
use tokio::sync::mpsc::channel;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();

    let token = CancellationToken::new();
    let tracker = TaskTracker::new();

    // Create a command channel between the key listener and the recorder so we
    // can start/stop the recording depending on the key state.
    let (tx, rx) = channel::<RecorderCommand>(32);

    let listener = KeyListener::new()?;
    let recorder = Recorder::new();

    // Ensure the transcription model is available.
    steno_daemon::model::ensure_parakeet_model().await?;

    // Spawn the key listener that will grab Ctrl+Super to trigger the recording of audio.
    // It will poll every 15 milliseconds for the recording trigger.
    let listener_task = tracker.spawn(listener.listen(tx, token.clone()));
    let recorder_task = tracker.spawn(recorder.listen(rx, token.clone()));
    tracker.close();

    let mut sigterm =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

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
