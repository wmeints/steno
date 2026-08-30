//! End-to-end dictation pipeline: a real Start → record → Stop cycle runs
//! through the actual recorder (live PipeWire capture), a scripted model, a
//! mock injection device, and the real [`Notifier`] on a session bus.
//! Asserts the desktop receives exactly the three dictation notifications,
//! in causal order.
//!
//! Requires a running PipeWire session. Run inside `dbus-run-session` for a
//! hermetic bus, or against the desktop's real session bus to also see the
//! notifications pop up on screen.

use std::time::Duration;

use anyhow::Result;
use parakeet_rs::{TimestampMode, Transcriber, TranscriptionResult};
use steno_daemon::notifications::{self, DictationEvent};
use steno_daemon::recorder::{Recorder, RecorderCommand};
use steno_daemon::uinput::{Device, Injector, KeyEvent};
use tokio::sync::mpsc::channel;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// Model that skips inference and always answers with fixed text.
struct ScriptedModel;

impl Transcriber for ScriptedModel {
    fn transcribe_samples(
        &mut self,
        _audio: Vec<f32>,
        _sample_rate: u32,
        _channels: u16,
        _mode: Option<TimestampMode>,
    ) -> parakeet_rs::Result<TranscriptionResult> {
        Ok(TranscriptionResult {
            text: "hello pipeline".to_string(),
            tokens: Vec::new(),
        })
    }
}

/// Injection sink that records nothing and never fails (no /dev/uinput).
#[derive(Clone)]
struct NullDevice;

impl Device for NullDevice {
    fn write_events(&mut self, _events: &[KeyEvent]) -> Result<()> {
        Ok(())
    }
}

fn spawn_dbus_monitor() -> Option<(std::process::Child, std::path::PathBuf)> {
    let log = std::env::temp_dir().join(format!("steno-pipe-dbus-{}.log", std::process::id()));
    let child = std::process::Command::new("dbus-monitor")
        .arg("interface='org.freedesktop.Notifications',member='Notify'")
        .stdout(std::fs::File::create(&log).ok()?)
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    Some((child, log))
}

#[tokio::test]
#[ignore = "requires a running PipeWire session (and dbus-monitor for the bus assertion)"]
async fn one_dictation_cycle_emits_three_notifications_in_order() {
    let _ = tracing_subscriber::fmt().try_init();

    // Subscribe before anything is sent, then let the match rule settle.
    let Some((mut monitor, log)) = spawn_dbus_monitor() else {
        eprintln!("dbus-monitor not installed; skipping");
        return;
    };
    tokio::time::sleep(Duration::from_millis(500)).await;

    let token = CancellationToken::new();
    let tracker = TaskTracker::new();

    // Long-lived loops are detached (not on the tracker): the tracker only
    // carries the recorder's fire-and-forget transcription tasks, so
    // `tracker.wait()` drains in-flight dictations and nothing else.
    let (notifier_tx, notifier_rx) = channel::<DictationEvent>(64);
    let (notifier, resource) = notifications::Notifier::connect().await;
    let driver_ct = token.clone();
    let driver = tokio::spawn(async move {
        if let Some(resource) = resource {
            notifications::serve_resource(resource, driver_ct).await;
        }
    });
    let loop_ct = token.clone();
    let notify_loop = tokio::spawn(notifier.listen(notifier_rx, loop_ct));

    let (inject_tx, inject_rx) = channel::<String>(16);
    let inj_ct = token.clone();
    let injector = tokio::spawn(
        Injector::with_notifier(NullDevice, notifier_tx.clone()).listen(inject_rx, inj_ct),
    );

    let model = std::sync::Arc::new(std::sync::Mutex::new(ScriptedModel));
    let recorder = Recorder::new(model, tracker.clone(), false, inject_tx, notifier_tx);
    let (cmd_tx, cmd_rx) = channel::<RecorderCommand>(32);
    let recorder_task = tokio::spawn(recorder.listen(cmd_rx, token.clone()));

    // A full cycle: press (Start), hold, release (Stop), then drain the
    // fire-and-forget transcription task.
    cmd_tx.send(RecorderCommand::Start).await.unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    cmd_tx.send(RecorderCommand::Stop).await.unwrap();
    tracker.close();
    tokio::time::timeout(Duration::from_secs(30), tracker.wait())
        .await
        .expect("pipeline drains");

    // Give the bus time to deliver, then settle and cancel.
    tokio::time::sleep(Duration::from_millis(500)).await;
    token.cancel();
    drop(cmd_tx);
    recorder_task.await.unwrap();
    injector.await.unwrap();
    notify_loop.await.unwrap();
    let _ = driver.await;
    let _ = monitor.kill();

    let captured = std::fs::read_to_string(&log).expect("read monitor log");
    let _ = std::fs::remove_file(&log);

    let calls: Vec<&str> = captured
        .lines()
        .filter(|l| l.contains("method call") && l.contains("member=Notify"))
        .collect();
    assert_eq!(calls.len(), 3, "exactly three Notify calls:\n{captured}");
    let started = captured
        .find(notifications::notification_body(
            &DictationEvent::RecordingStarted,
        ))
        .expect("recording notification");
    let transcribing = captured
        .find(notifications::notification_body(
            &DictationEvent::TranscriptionStarted,
        ))
        .expect("transcription notification");
    let finished = captured
        .find(notifications::notification_body(
            &DictationEvent::DictationFinished,
        ))
        .expect("finished notification");
    assert!(
        started < transcribing && transcribing < finished,
        "causal order violated"
    );
}
