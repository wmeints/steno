use anyhow::Result;
use std::time::Duration;
use steno_daemon::listener::KeyListener;
use steno_daemon::recorder::{Recorder, RecorderCommand};
use tokio::sync::mpsc::channel;
use tokio::{
    signal::unix::{SignalKind, signal},
    time::timeout,
};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

async fn shutdown_signal() {
    let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM handler");

    tokio::select! {
        _ = tokio::signal::ctrl_c() => tracing::info!("received SIGINT"),
        _ = sigterm.recv() => tracing::info!("received SIGTERM")
    }
}

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

    // Spawn the key listener that will grab Ctrl+Super to trigger the recording of audio.
    // It will poll every 15 milliseconds if the recording trigger is active or not.
    tracker.spawn(listener.listen(tx, token.clone()));
    tracker.spawn(recorder.listen(rx, token.clone()));
    tracker.close();

    tracing::info!("daemon active");

    shutdown_signal().await;
    token.cancel();

    let timed_out = timeout(Duration::from_secs(30), tracker.wait())
        .await
        .is_err();

    if timed_out {
        tracing::warn!("shutdown timed out, forcing exit");
    }

    Ok(())
}
