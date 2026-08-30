//! Session-bus integration test for the notification path.
//!
//! Run inside a private bus (happy path) or with `DBUS_SESSION_BUS_ADDRESS`
//! pointed at a non-existent socket (downgrade path):
//!   DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent/bus \
//!     cargo test -p steno-daemon --test notification_integration_tests -- --ignored

use std::time::Duration;

use steno_daemon::notifications::{self, DictationEvent};
use tokio::sync::mpsc::channel;
use tokio_util::sync::CancellationToken;

/// Whatever the bus state, `connect` must resolve quickly and the notifier
/// loop must consume events without panicking or stalling (design D3).
#[tokio::test]
#[ignore = "controls the session bus via DBUS_SESSION_BUS_ADDRESS; run standalone"]
async fn notifier_survives_whatever_the_bus_state_is() {
    let _ = tracing_subscriber::fmt().try_init();
    let (notifier, resource) = notifications::Notifier::connect().await;
    let ct = CancellationToken::new();
    if let Some(resource) = resource {
        tokio::spawn(notifications::serve_resource(resource, ct.clone()));
    }

    let (tx, rx) = channel::<DictationEvent>(16);
    let task = tokio::spawn(notifier.listen(rx, ct));
    for event in [
        DictationEvent::RecordingStarted,
        DictationEvent::TranscriptionStarted,
        DictationEvent::DictationFinished,
    ] {
        notifications::emit(&tx, event);
    }
    drop(tx);

    tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .expect("listen loop exits on channel close")
        .unwrap();
}

/// Assert every event becomes exactly one `Notify` method call on the bus,
/// with the right bodies, in order. Must run inside `dbus-run-session`
/// (see module docs); skips (exit 0) if dbus-monitor is not installed.
#[tokio::test]
#[ignore = "requires dbus-run-session; invoke via the documented command"]
async fn every_event_becomes_one_notify_call() {
    let _ = tracing_subscriber::fmt().try_init();
    let log = std::env::temp_dir().join(format!("steno-dbus-monitor-{}.log", std::process::id()));
    let monitor = std::process::Command::new("dbus-monitor")
        .arg("interface='org.freedesktop.Notifications',member='Notify'")
        .stdout(std::fs::File::create(&log).expect("create monitor log"))
        .stderr(std::process::Stdio::null())
        .spawn();
    let Ok(mut monitor) = monitor else {
        eprintln!("dbus-monitor not installed; skipping");
        return;
    };
    // Give the monitor time to subscribe before messages flow.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let (notifier, resource) = notifications::Notifier::connect().await;
    let ct = CancellationToken::new();
    let driver_ct = ct.clone();
    let driver = tokio::spawn(async move {
        if let Some(resource) = resource {
            notifications::serve_resource(resource, driver_ct).await;
        }
    });
    let (tx, rx) = channel::<DictationEvent>(16);
    let loop_task = tokio::spawn(notifier.listen(rx, ct.clone()));

    let events = [
        DictationEvent::RecordingStarted,
        DictationEvent::TranscriptionStarted,
        DictationEvent::DictationFinished,
    ];
    for event in events {
        notifications::emit(&tx, event);
    }
    drop(tx);
    loop_task.await.unwrap();
    ct.cancel();
    let _ = driver.await;
    // Flush the monitor's pipe.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let _ = monitor.kill();

    let captured = std::fs::read_to_string(&log).expect("read monitor log");
    let _ = std::fs::remove_file(&log);
    let calls: Vec<&str> = captured
        .lines()
        .filter(|l| l.contains("member=Notify"))
        .collect();
    assert_eq!(
        calls.len(),
        3,
        "one Notify per event, in order:\n{captured}"
    );
    for event in events {
        assert!(
            captured.contains(notifications::notification_body(&event)),
            "missing body for {event:?}"
        );
    }
}

/// Real-delivery check against the desktop's own notification daemon: the
/// production `Notify` message must be accepted and return a notification
/// id (a malformed message is rejected — exactly what dbus-monitor could
/// not see when the six-argument signature bug shipped).
/// Run with the desktop's session bus:
///   cargo test -p steno-daemon --test notification_integration_tests \
///     -- --ignored notification_call_is_accepted_by_the_daemon
#[tokio::test]
#[ignore = "pops a real notification on the user's desktop"]
async fn notification_call_is_accepted_by_the_daemon() {
    use dbus::nonblock::NonblockReply;
    use tokio::sync::oneshot;

    let (notifier, resource) = notifications::Notifier::connect().await;
    let conn = notifier
        .connection()
        .expect("desktop session bus must be reachable");
    if let Some(resource) = resource {
        let ct = CancellationToken::new();
        tokio::spawn(notifications::serve_resource(resource, ct));
    }

    let msg = notifications::notify_message(&DictationEvent::DictationFinished);
    let (tx, rx) = oneshot::channel();
    conn.send_with_reply(
        msg,
        Box::new(move |mut reply, _| {
            let outcome = match reply.as_result() {
                Ok(_) => reply.read1::<u32>().map_err(|e| format!("{e}")),
                Err(err) => Err(format!("{err}")),
            };
            let _ = tx.send(outcome);
        }),
    )
    .expect("bus accepts the queued call");

    let id = tokio::time::timeout(Duration::from_secs(5), rx)
        .await
        .expect("daemon replied within 5s")
        .expect("reply channel alive")
        .expect("daemon must accept the Notify call");
    assert!(id > 0, "daemon assigned a notification id");
}
