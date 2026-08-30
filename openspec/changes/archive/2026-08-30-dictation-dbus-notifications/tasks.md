# Tasks: dictation-dbus-notifications

## 1. Notification module

- [x] 1.1 Add the `dbus` (vendored) and `dbus-tokio` crates to `crates/steno-daemon/Cargo.toml` and verify `cargo check -p steno-daemon` succeeds.
- [x] 1.2 Create `crates/steno-daemon/src/notifications.rs` with `DictationEvent` enum and the pure `notification_body` mapping (design D4); write failing unit tests for the three event→body mappings first (red-green-refactor), then make them pass via `cargo test -p steno-daemon notifications`.
- [x] 1.3 Implement `Notifier` with `dbus-tokio` (design D2/D3): probe the session bus via `blocking::Connection::new_session()` in `spawn_blocking`, wrap with `dbus_tokio::new_resource`, drive the `Resource` future, and send `org.freedesktop.Notifications.Notify` (app name "steno", design D1/D4) per event from an async loop draining an `mpsc::Receiver<DictationEvent>`; a connect failure logs an error and drains/discards instead. Verify with `cargo check` and a unit test that the no-bus mode consumes events without panicking.
- [x] 1.4 Export the module from `crates/steno-daemon/src/lib.rs` and verify `cargo test -p steno-daemon` passes.

## 2. Emit points

- [x] 2.1 Add `notifier_tx: Sender<DictationEvent>` to `Recorder` (constructor + `with_wav_dir`), send `RecordingStarted` in `begin_capture` and `TranscriptionStarted` in `transcribe` (design D6); update existing recorder tests' constructors and verify each new emit with a test asserting the event arrives on a test channel (capture-error and empty-sample paths emit no transcription event).
- [x] 2.2 In `Injector::listen`, send `DictationFinished` after each successful `inject` call only (design D5); verify with a test using the mock `Device` that success emits once per text and a failed injection emits nothing.
- [x] 2.3 Wire in `crates/steno-daemon/src/main.rs`: connect/spawn the notifier before the recorder and injector tasks, pass clones of `notifier_tx` (design D3: daemon starts even when the bus is absent). Verify `cargo run` (no bus) logs the error and the daemon still reaches "daemon active"; with a desktop session it starts normally.

## 3. Integration verification

- [x] 3.1 End-to-end manual check: in a desktop session with `busctl --user monitor` (or on-screen), press Ctrl+Super, speak, release; confirm the three notifications appear in order recording → transcription → finished, and that a no-speech capture shows only the recording notification (spec scenarios "Full dictation cycle" and "Capture produces no audio").
- [x] 3.2 Run `cargo clippy --all-targets --all-features -- -D warnings` and `cargo fmt --check`; verify both exit 0.
