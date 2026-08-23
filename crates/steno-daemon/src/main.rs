use anyhow::Result;
use kbd::hotkey::Modifier;
use kbd_global::{backend::Backend, manager::HotkeyManager};
use std::time::Duration;
use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::interval;
use tokio::{
    signal::unix::{SignalKind, signal},
    time::timeout,
};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

#[derive(Debug)]
enum Command {
    Start,
    Stop,
}

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
    let (tx, rx) = channel::<Command>(32);

    // Build the hotkey manager against the evdev driver and make sure to grab
    // the device because wayland will have it grabbed and we can't listen if
    // that's the case!
    let hotkey_mgr = HotkeyManager::builder()
        .backend(Backend::Evdev)
        .grab()
        .build()?;

    // Spawn the key listener that will grab Ctrl+Super to trigger the recording of audio.
    // It will poll every 15 milliseconds if the recording trigger is active or not.
    tracker.spawn(key_listener(hotkey_mgr, tx, token.clone()));
    tracker.spawn(recorder(rx, token.clone()));
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

async fn key_listener(
    hotkey_mgr: HotkeyManager,
    tx: Sender<Command>,
    token: CancellationToken,
) -> Result<()> {
    let mut ticker = interval(Duration::from_millis(15));
    let mut is_activated = false;

    loop {
        let modifiers = hotkey_mgr.active_modifiers()?;

        if modifiers.contains(Modifier::Ctrl) && modifiers.contains(Modifier::Super) {
            // Activate the recording mode, and send the start command to the recorder.
            // The recorder will start capturing audio.
            if !is_activated {
                is_activated = true;
                tx.send(Command::Start).await?;
            }
        } else {
            // Deactivate the recording mode, and send the stop command to the recorder.
            // The recorder will handle the transcription after this.
            if is_activated {
                is_activated = false;
                tx.send(Command::Stop).await?;
            }
        }

        tokio::select! {
            _ = token.cancelled() => break,
            _ = ticker.tick() => {}
        }
    }

    Ok(())
}

async fn recorder(mut rx: Receiver<Command>, token: CancellationToken) -> Result<()> {
    let mut is_recording = false;

    loop {
        tokio::select! {
            _ = token.cancelled() => break,
            Some(msg) = rx.recv() => {
                if let Command::Start = msg {
                    is_recording = true;
                    tracing::info!("Start recording");
                }

                if let Command::Stop = msg && is_recording {
                    is_recording = false;
                    tracing::info!("Stop recording");
                }
            },
        }
    }

    Ok(())
}
