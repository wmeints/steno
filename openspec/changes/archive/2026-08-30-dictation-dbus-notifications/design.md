# Design: dictation-dbus-notifications

## Context

The daemon is a Tokio app whose capture path already follows an
owner-task-per-resource pattern: `KeyListener → Recorder → Injector`, each
bridged by an `mpsc` channel, with the `Injector` exclusively owning
`/dev/uinput` (see `crates/steno-daemon/src/uinput.rs`). Event points:

- Recording starts: `Recorder::begin_capture` (after capture reaches
  streaming state).
- Transcription starts: `Recorder::transcribe` (samples non-empty, handed to
  the model task).
- Injection completes: after `Injector::inject` returns Ok in the injector
  task loop (`uinput.rs listen`).

Constraints: the `dbus` crate is synchronous; the daemon's pipeline is all
async, so D-Bus must not be touched with blocking calls on runtime worker
threads (the issue names the `dbus` crate; a Tokio binding keeps the
pipeline uniform). A dead notification daemon must never degrade dictation
(specs/dictation-notifications).

## Goals / Non-Goals

**Goals:**
- Three lifecycle events delivered as freedesktop notifications on the
  session bus.
- Isolated, ordered, non-blocking delivery; no effect on capture/injection.
- Unit-testable event→notification mapping without a live bus.

**Non-Goals:**
- Owning a bus name or exposing daemon state via D-Bus properties/methods.
- Progress/percentage notifications or custom icons/themes.
- Fallback notification channels (notify-send subprocess, tray).

## Decisions

### D1: Dedicated async notifier task over one event channel

A `Notifier` task owns the D-Bus connection and receives `DictationEvent`
values over an mpsc channel — the same shape as `Injector`. The capture
path only does `tx.send(event)`; all bus I/O happens on this one task, so
event order is preserved and no other code learns about D-Bus.

Alternative: call D-Bus inline at each emit point — rejected: bus error
handling would scatter across recorder/injector, and a wedged bus would
stall the capture path.

### D2: `dbus-tokio` binds the `dbus` crate to the runtime

`dbus-tokio` (same dbus-rs family, the official Tokio binding) wraps a
`dbus::blocking::Connection` into a non-blocking `dbus::Connection` plus a
`Resource` future. At startup: probe the session bus with
`blocking::Connection::new_session()` inside one `spawn_blocking` (a
one-time connect, never on a worker), convert via `dbus_tokio::new_resource`,
spawn the `Resource` driver as a task, and fire notifications with
`Connection::send(msg)` — non-blocking queueing, reply ignored.

Alternatives considered: raw sync `dbus` on a `spawn_blocking` thread —
rejected in favor of the async-native binding per user direction;
`zbus` — rejected: heavier dependency tree (macros, futures stack) for a
fire-and-forget use case, and the issue names the `dbus` crate.

### D3: Bus startup failure degrades to no-op

At daemon startup, connecting to the session bus is attempted; on failure
(missing `DBUS_SESSION_BUS_ADDRESS`, headless/SSH session) the daemon logs
`error!("D-Bus session bus unavailable: …; notifications disabled")` and
spawns a task that drains and discards events. Rationale: notifications are
informational; unlike `/dev/uinput`, their absence does not break the core
function, and failing startup would regress headless environments.

### D4: Event enum + pure mapping, tested without a bus

```rust
pub enum DictationEvent { RecordingStarted, TranscriptionStarted, DictationFinished }
```

`fn notification_body(event: &DictationEvent) -> &'static str` is a pure
function; unit tests cover the mapping. `fn notify_message(event) ->
dbus::Message` builds the `org.freedesktop.Notifications.Notify` call with
the exact spec signature `susssasa{sv}i` **in spec argument order** (app_name
"steno", replaces_id 0, empty icon, event text in the **summary** — always
rendered — empty body, empty actions, empty hints, expire -1). Two traps
verified against the live COSMIC daemon and pinned by tests: an empty
`Vec<(String, Variant)>` marshals as `a(sv)` not `a{sv}` (use
`dbus::arg::Dict`), and a malformed call is rejected by the daemon while
`dbus-monitor` still shows it as delivered — so the notifier awaits its
reply via `send_with_reply` and logs rejections. No `close`/timeout
juggling — default expiry per notification keeps it boring.

### D5: Completion signal from the injector

`Injector` gains a `notifier_tx: Sender<DictationEvent>` field (constructor
parameter); after each successful `self.inject(&text)` in `listen` it sends
`DictationFinished`. Failed injections do NOT emit finished (text never
fully landed). Alternative: wrap each request with a `oneshot` reply channel
— rejected: completion goes to one known consumer (the notifier), so a
direct `Sender` clone is simpler and matches the existing `inject_tx` style
in `Recorder`.

### D6: Recorder emit points

`Recorder` gains a `notifier_tx: Sender<DictationEvent>` (constructor
parameter alongside `inject_tx`), sending:
- `RecordingStarted` in `begin_capture` (after the streaming-ready check),
- `TranscriptionStarted` at the top of `transcribe` (before spawning the
  inference task; the zero-sample case already returns earlier, matching the
  "no audio" spec scenario).

Send failures log at `warn` (closed channel must never abort the pipeline).

## Risks / Trade-offs

- [Notification spam: three popups per dictation may annoy] → short
  expiry + terse bodies; severity tuned to "normal". Revisit with
  replaces_id if user feedback demands a single updating notification.
- [`send` only queues; a wedged bus never flushes] → capture/injection are
  unaffected (events queue on the mpsc); queued notifications are lost at
  shutdown — acceptable per the best-effort spec requirement.
- [Two dicts racing: finished-before-started inversion] → prevented by
  design: recorder/injector emit in causal order into one FIFO channel
  consumed by one notifier task.
- [Session-bus address in systemd-user environments] → `new_session()`
  reads `DBUS_SESSION_BUS_ADDRESS`, the same mechanism `notify-send` uses;
  verified manually in acceptance.
- [`dbus-tokio` is low-churn (last release 2023)] → API is stable and tiny
  (`new_resource` + `Resource`); vendored `dbus` keeps the build hermetic.

## Migration Plan

Purely additive; no data or interface migration. Rollback = revert commits.
Deploy: `cargo build`, run daemon under a normal desktop session, press/release
Ctrl+Super, observe three notifications via `busctl --user monitor` or on screen.

## Open Questions

- None blocking. (Icon choice and whether to also expose an `org.steno`
  property interface are deferred; neither affects specs or tasks.)
