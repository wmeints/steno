# Proposal: dictation-dbus-notifications

## Why

The daemon captures, transcribes, and injects text silently — the user has no
feedback about which phase a dictation is in, and other applications have no
way to observe dictation state. Linux's standard channel for user-visible
state changes is D-Bus, so the daemon should announce dictation lifecycle
events there (GitHub issue #4).

## What Changes

- Add a D-Bus notification module to `steno-daemon` using the `dbus` crate
  with its `dbus-tokio` Tokio binding.
- On the session bus, the daemon sends freedesktop desktop notifications
  (`org.freedesktop.Notifications.Notify`) for three dictation events:
  1. **Recording started** — when capture begins after Ctrl+Super is pressed.
  2. **Transcription started** — when the key is released and audio is handed
     to Parakeet for transcription.
  3. **Dictation finished** — when transcription text has been fully written
     to the uinput device.
- The event stream flows through a dedicated notification task (channel in,
  D-Bus out), mirroring the existing injector-task pattern, so notification
  failures never block or kill the capture/transcription path.
- If the session bus is unavailable at daemon startup, the daemon logs an
  error and continues — notifications are best-effort, unlike `/dev/uinput`
  which remains a hard startup requirement.

## Capabilities

### New Capabilities

- `dictation-notifications`: D-Bus desktop notifications emitted by the
  daemon for the three dictation lifecycle events, including event ordering,
  best-effort delivery semantics, and session-bus availability behavior.

### Modified Capabilities

- `text-injection`: the injector gains a spec-level obligation to report
  completion — once all keystrokes of an injected text have been written to
  the device, it must emit the "dictation finished" event downstream.

## Impact

- New dependencies: `dbus` crate (vendored build, no system `libdbus-1-dev`
  needed) and `dbus-tokio` (Tokio binding — the notification task is an
  async task on the existing runtime).
- Code: new `crates/steno-daemon/src/notifications.rs`; wiring in
  `crates/steno-daemon/src/main.rs`; emit points in
  `crates/steno-daemon/src/recorder.rs` (recording/transcription events) and
  `crates/steno-daemon/src/uinput.rs` (completion event).
- No change to capture timing, transcription, or injection behavior beyond
  the completion signal.
