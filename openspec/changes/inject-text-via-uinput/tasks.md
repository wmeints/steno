# Tasks: inject-text-via-uinput

## 1. Dependency and module scaffold

- [x] 1.1 Add the `uinput` crate to `crates/steno-daemon/Cargo.toml` and declare `pub mod uinput;` in `lib.rs`. Verify: `cargo check -p steno-daemon` compiles and `cargo tree -p steno-daemon | sed -n '/uinput/p'` shows the new dep.

## 2. Pure text translation (OS-free, TDD)

- [x] 2.1 In `uinput.rs`, write failing unit tests for `translate()`: "hello world" keycodes, mixed case "Hi! It's 3pm." produces shift press/release only around shifted chars, newline → Enter, tab → Tab. Verify: `cargo test -p steno-daemon translate` fails (function absent).
- [x] 2.2 Implement `translate(text: &str) -> Translation` (events + skipped chars) with the static char→(Keycode, Shift) table covering letters, digits, space/tab/newline, and the punctuation set from the spec. Verify: 2.1 tests pass.
- [x] 2.3 Add failing-then-passing unit test: unsupported chars (emoji, "é") are skipped and reported, remaining chars still translated (spec: "great 🎉 thanks" → "great  thanks" + one skipped report). Keep `translate` cognitive complexity ≤ 10 by extracting helpers if needed. Verify: `cargo clippy --all-targets -- -D warnings` clean, tests pass.

## 3. Device-backed injector

- [x] 3.1 Define a `Device` trait (`fn write_events(&self, events: &[input_event]) -> Result<()>` over the `uinput` crate's event type) implemented by a real wrapper around `uinput::VirtualDevice`, and an `Injector` struct holding the device. Verify: `cargo check` passes; real impl opens via `uinput::default()` + `UInput::build()` with keyboard keycodes + `with_name("steno-virtual-keyboard")` and `enable()`.
- [x] 3.2 Implement `Injector::inject(&self, text: &str)`: translate, write each character's event group in one `write_events` call, log `tracing::warn!` per skipped char. Verify: unit tests with a recording mock `Device` assert exact event sequences for "Hi!" and shift release ordering (spec scenarios).
- [x] 3.3 Implement the injector task loop `Injector::listen(rx: mpsc::Receiver<String>, ct: CancellationToken)`: drain channel FIFO, one injection at a time, exit on cancel and drop the device. Verify: mock-device test injects two queued strings and asserts full first-text-before-second-text ordering.

## 4. Daemon wiring

- [x] 4.1 `main.rs`: construct the real device before spawning tasks; on open failure return `Err` (daemon exits non-zero, error names `/dev/uinput`). Create `mpsc::channel::<String>(16)`, spawn the injector task via `TaskTracker`. Verify: `steno-daemon` on a box without `/dev/uinput` write access logs the device error and exits 1.
- [x] 4.2 Pass the `Sender<String>` into `Recorder`; on `RecorderCommand::Stop` leave a clearly-marked call site (`// transcription output -> inject_tx`) sending nothing yet (transcription not wired). Verify: existing recorder tests still pass; `cargo test -p steno-daemon` green.

## 5. Verification

- [x] 5.1 Manual workstation smoke test: build, ensure `/dev/uinput` is writable (udev rule or sudo tee if missing), focus a text editor, run a debug binary/test harness that sends "Hello, Steno! Line two." through the injector task. Verify: text appears in the editor exactly; `grep steno /proc/bus/input/devices` shows the device while running and not after exit.
- [x] 5.2 Full gate: `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test` exit 0; `cargo fmt --check` clean.
