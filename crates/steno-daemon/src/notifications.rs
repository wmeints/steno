//! Dictation lifecycle notifications over D-Bus.
//!
//! The daemon emits [`DictationEvent`]s from the capture path into an mpsc
//! channel; a single async [`Notifier`] task owns the session-bus connection
//! (via `dbus-tokio`) and turns each event into a freedesktop desktop
//! notification. Delivery is best-effort: a missing or wedged bus must
//! never affect recording, transcription, or injection.

use std::fmt::Display;
use std::sync::Arc;
use std::time::Duration;

use dbus::Message;
use dbus::nonblock::{NonblockReply, SyncConnection};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio_util::sync::CancellationToken;

/// A dictation lifecycle event to announce to the desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictationEvent {
    RecordingStarted,
    TranscriptionStarted,
    DictationFinished,
}

/// The notification body text for an event. Pure so the mapping can be
/// unit-tested without a bus.
pub fn notification_body(event: &DictationEvent) -> &'static str {
    match event {
        DictationEvent::RecordingStarted => "● Recording…",
        DictationEvent::TranscriptionStarted => "Transcribing…",
        DictationEvent::DictationFinished => "✓ Dictation complete",
    }
}

/// The application name shown with every notification.
const APP_NAME: &str = "steno";

/// How long the notifier waits for the session bus probe at startup.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Build the `org.freedesktop.Notifications.Notify` method call for an
/// event. Signature is `susssasa{sv}i`: app name, replaces id, icon,
/// actions, summary, body, hints, expire timeout — a wrong arg count is
/// rejected by the notification daemon, so tests pin the full signature.
/// Pure so message construction is testable without a bus.
pub fn notify_message(event: &DictationEvent) -> Message {
    let actions: Vec<String> = Vec::new();
    // The transient hint (BOOLEAN, spec >= 1.2) makes the daemon bypass its
    // persistence capability, so dictation status never sticks around in
    // the notification list. Concrete `Variant<bool>` keeps the iterator
    // Clone (required by Dict's Append impl).
    let hints = vec![("transient".to_string(), dbus::arg::Variant(true))];
    Message::new_method_call(
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
        "Notify",
    )
    .expect("valid bus/path/interface/member")
    // Spec order: app_name, replaces_id, app_icon, summary, body, actions,
    // hints, expire_timeout. Event text rides in the summary line (always
    // rendered); the body stays empty. `Dict` marshals hints as a{sv}; a
    // plain Vec<(String, Variant)> becomes a(sv) and the daemon rejects it.
    .append3(APP_NAME, 0u32, "")
    .append3(notification_body(event), "", &actions)
    .append2(dbus::arg::Dict::new(hints), -1i32)
}

/// Owns the session-bus connection and delivers one notification per event.
///
/// `Notifier::connect` is the daemon constructor; when no session bus was
/// reachable at startup the notifier drains and discards events (design D3).
pub struct Notifier {
    connection: Option<Arc<SyncConnection>>,
}

impl Notifier {
    /// The session-bus connection, when one was established. Integration
    /// tests use it to verify the daemon accepts production notifications.
    pub fn connection(&self) -> Option<Arc<SyncConnection>> {
        self.connection.clone()
    }
}

impl Notifier {
    /// A notifier that drains and discards every event — what the daemon
    /// runs with (design D3), and what tests use to verify the loop.
    pub fn disabled() -> Self {
        Self { connection: None }
    }

    /// Probe the session bus off the runtime workers (the connect itself
    /// blocks), and hand back the notifier plus the `IOResource` future the
    /// caller must spawn to drive the bus fd. Never fails: an unreachable
    /// bus downgrades to discard mode with one logged error.
    pub async fn connect() -> (
        Self,
        Option<dbus_tokio::connection::IOResource<SyncConnection>>,
    ) {
        let probe = tokio::task::spawn_blocking(dbus_tokio::connection::new_session_sync);
        match tokio::time::timeout(CONNECT_TIMEOUT, probe).await {
            Ok(Ok(Ok((resource, connection)))) => (
                Self {
                    connection: Some(connection),
                },
                Some(resource),
            ),
            Ok(Ok(Err(err))) => {
                disabled(&err);
                (Self::disabled(), None)
            }
            Ok(Err(join)) => {
                disabled(&format!("bus probe task ended: {join:?}"));
                (Self::disabled(), None)
            }
            Err(_) => {
                disabled(&format!(
                    "session bus probe timed out after {CONNECT_TIMEOUT:?}"
                ));
                (Self::disabled(), None)
            }
        }
    }

    /// Send a notification per event until the channel closes or the token
    /// is cancelled.
    pub async fn listen(self, mut rx: Receiver<DictationEvent>, ct: CancellationToken) {
        while let Some(event) = Self::next_event(&mut rx, &ct).await {
            self.deliver(&event);
        }
    }

    /// Queue one notification, or discard it in disabled mode. The call is
    /// fire-and-forget, but the reply is observed through a callback so a
    /// daemon-side rejection (bad signature, policy denial) is logged
    /// instead of silently dropped (best-effort).
    fn deliver(&self, event: &DictationEvent) {
        let Some(conn) = &self.connection else {
            tracing::debug!("notifications disabled, discarding {event:?}");
            return;
        };
        let msg = notify_message(event);
        let logged = *event;
        if conn
            .send_with_reply(msg, Box::new(move |reply, _conn| log_reply(&logged, reply)))
            .is_err()
        {
            tracing::warn!("failed to queue dictation notification: {logged:?}");
        }
    }

    /// The next event, or None once cancellation is requested.
    async fn next_event(
        rx: &mut Receiver<DictationEvent>,
        ct: &CancellationToken,
    ) -> Option<DictationEvent> {
        tokio::select! {
            biased;
            event = rx.recv() => event,
            () = ct.cancelled() => None,
        }
    }
}

/// Log the startup downgrade once (design D3).
fn disabled(reason: &dyn Display) {
    tracing::error!("D-Bus session bus unavailable: {reason}; notifications disabled");
}

/// Poll the dbus-tokio resource until cancellation or connection loss. A
/// lost bus only disables notifications; the daemon keeps working (D3).
pub async fn serve_resource(
    resource: dbus_tokio::connection::IOResource<SyncConnection>,
    ct: CancellationToken,
) {
    tokio::select! {
        () = ct.cancelled() => {}
        err = resource => log_resource_end(err),
    }
}

/// Surface daemon errors for a notification we sent. Success replies carry
/// the assigned id, which is not needed.
fn log_reply(event: &DictationEvent, mut reply: dbus::Message) {
    if let Err(err) = reply.as_result() {
        tracing::warn!("notification daemon rejected {event:?}: {err}");
    }
}

/// Log a lost D-Bus connection (notifications stop, daemon continues).
fn log_resource_end(err: dbus_tokio::connection::IOResourceError) {
    tracing::warn!("D-Bus connection ended: {err}; notifications disabled");
}

/// Hand an event to the notification task. A closed or full channel logs a
/// warning and is ignored — notifications must never abort the capture path.
pub fn emit(notifier_tx: &Sender<DictationEvent>, event: DictationEvent) {
    if notifier_tx.try_send(event).is_err() {
        tracing::warn!("dropping dictation notification {event:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_event_has_a_distinct_body() {
        let bodies = [
            notification_body(&DictationEvent::RecordingStarted),
            notification_body(&DictationEvent::TranscriptionStarted),
            notification_body(&DictationEvent::DictationFinished),
        ];
        assert!(
            bodies.iter().all(|b| !b.is_empty()),
            "bodies must be non-empty"
        );
        let mut unique = bodies.to_vec();
        unique.sort_unstable();
        let count = unique.len();
        unique.dedup();
        assert_eq!(unique.len(), count, "bodies must be pairwise distinct");
    }

    #[test]
    fn notify_message_targets_freedesktop_notifications() {
        let msg = notify_message(&DictationEvent::RecordingStarted);
        assert_eq!(msg.interface().unwrap(), "org.freedesktop.Notifications");
        assert_eq!(msg.member().unwrap(), "Notify");
        assert_eq!(msg.destination().unwrap(), "org.freedesktop.Notifications");
        assert_eq!(msg.path().unwrap(), "/org/freedesktop/Notifications");
    }

    /// The full `Notify` argument tuple, in spec order (susssasa{sv}i):
    /// app_name, replaces_id, app_icon, summary, body, actions, hints, expire.
    type NotifyArgs = (
        String,
        u32,
        String,
        String,
        String,
        Vec<String>,
        Vec<(String, dbus::arg::Variant<Box<dyn dbus::arg::RefArg>>)>,
        i32,
    );

    /// Read a boolean hint off the message's item tree. (`read_all` into a
    /// `Vec<(String, Variant)>` decodes dict entries as nothing — they are
    /// `{sv}` on the wire, not `(sv)` — so assert through `get_items`.)
    fn hint_bool(msg: &dbus::Message, key: &str) -> Option<bool> {
        use dbus::arg::messageitem::MessageItem::*;
        msg.get_items().into_iter().find_map(|item| match item {
            Dict(dict) => dict.into_vec().into_iter().find_map(|(k, v)| match (k, v) {
                (Str(name), Variant(inner)) if name == key && matches!(*inner, Bool(true)) => {
                    Some(true)
                }
                _ => None,
            }),
            _ => None,
        })
    }

    fn notify_args(event: &DictationEvent) -> NotifyArgs {
        notify_message(event)
            .read_all()
            .expect("args must match the Notify signature")
    }

    #[test]
    fn notify_message_decodes_as_the_full_notify_signature() {
        // read_all succeeds only if all eight args (susssasa{sv}i) are
        // present with the right types; the old six-arg message failed here.
        let _ = notify_args(&DictationEvent::RecordingStarted);
    }

    #[test]
    fn notify_message_summary_carries_the_event_text() {
        let args = notify_args(&DictationEvent::DictationFinished);
        assert_eq!(
            args.3,
            notification_body(&DictationEvent::DictationFinished)
        );
        assert_eq!(args.4, "");
    }

    #[test]
    fn notify_message_uses_defaults_for_the_rest() {
        let args = notify_args(&DictationEvent::TranscriptionStarted);
        assert_eq!(args.0, APP_NAME);
        assert_eq!(args.2, "");
        assert_eq!((args.1, args.7), (0, -1));
        assert!(args.5.is_empty());
        // Hints carry the transient flag only; see the dedicated test.
    }

    #[test]
    fn notify_message_marks_notifications_transient() {
        // Transient = bypass the daemon's persistence: no leftover entries
        // in the notification list after the popup expires.
        assert_eq!(
            hint_bool(
                &notify_message(&DictationEvent::RecordingStarted),
                "transient"
            ),
            Some(true)
        );
    }

    #[tokio::test]
    async fn emit_delivers_events_in_order() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        emit(&tx, DictationEvent::RecordingStarted);
        emit(&tx, DictationEvent::TranscriptionStarted);
        emit(&tx, DictationEvent::DictationFinished);
        assert_eq!(rx.recv().await, Some(DictationEvent::RecordingStarted));
        assert_eq!(rx.recv().await, Some(DictationEvent::TranscriptionStarted));
        assert_eq!(rx.recv().await, Some(DictationEvent::DictationFinished));
    }

    #[tokio::test]
    async fn emit_on_closed_channel_does_not_panic() {
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        drop(rx);
        emit(&tx, DictationEvent::RecordingStarted); // must not panic
    }

    #[tokio::test]
    async fn disabled_notifier_drains_events_without_panicking() {
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        let ct = CancellationToken::new();
        let task = tokio::spawn(Notifier::disabled().listen(rx, ct.clone()));
        emit(&tx, DictationEvent::RecordingStarted);
        emit(&tx, DictationEvent::DictationFinished);
        drop(tx);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("drain-and-discard loop exits on channel close")
            .unwrap();
    }

    #[tokio::test]
    async fn listen_exits_on_cancel() {
        let (_tx, rx) = tokio::sync::mpsc::channel(8);
        let ct = CancellationToken::new();
        let cancel = ct.clone();
        let task = tokio::spawn(Notifier::disabled().listen(rx, ct));
        cancel.cancel();
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("listen exits on cancel")
            .unwrap();
    }
}
