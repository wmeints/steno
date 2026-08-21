## 1. Dependency and scaffolding

- [x] 1.1 Add a global key-capture dependency to `crates/steno-daemon/Cargo.toml` (candidate: an `evdev`/Wayland global keyboard source that works across desktops). Verify: `cargo build` in `crates/steno-daemon` succeeds with the new dependency.
- [x] 1.2 Add a logging dependency to `crates/steno-daemon/Cargo.toml` for the press/release log output. Verify: `cargo build` succeeds and the daemon can emit a log line to a greppable channel.

## 2. Capture-key state model

- [x] 2.1 Implement a state type tracking the pressed state of each modifier (`Ctrl`, `Super`) and a derived `active` boolean (`ctrl && super`). Verify: a unit test sets `ctrl`/`super` independently and asserts `active` is true only when both are true.
- [x] 2.2 Implement transition detection comparing the current `active` against the previous value, emitting exactly one press on false→true and one release on true→false. Verify: a unit test drives the sequence none→ctrl→super and asserts a single press; and none→both→super-only (drop ctrl) and both→ctrl-only (drop super) each assert a single release.
- [x] 2.3 Confirm repeated press/release cycles emit press then release for each cycle, in order. Verify: a unit test drives three full cycles and asserts the emitted event sequence is press, release, press, release, press, release.

- [x] 3.1 Log a distinct press entry when a capture-key press transition is detected. Verify: a unit test against a log sink asserts a `pressed` entry is written on the false→true transition.
- [x] 3.2 Log a distinct release entry when a capture-key release transition is detected. Verify: a unit test against a log sink asserts a `released` entry is written on the true→false transition.
- [x] 3.3 Ensure press and release entries are distinguishable and in order in the daemon's log output. Verify: run `cargo test` in `crates/steno-daemon`; all capture-key tests pass.

## 4. Global key-capture wiring

- [x] 4.1 Implement a key-source abstraction (trait) behind which the real OS capture binding and the synthetic test stream both live, so the state machine is testable without the OS binding. Verify: the same state-machine tests pass against the synthetic source; a compile check confirms the real binding implements the trait.
- [ ] 4.2 Wire the real global key-capture binding in `crates/steno-daemon/src/main.rs` to feed modifier press/release events into the state model, replacing the placeholder `Hello, world!` body. Verify: running the daemon and physically holding <kbd>Ctrl</kbd>+<kbd>Super</kbd>+<kbd>Space</kbd> then releasing produces a press and a release entry in the log output (record how to reproduce; note any permission/`input` group requirement observed).
- [ ] 4.3 Validate the binding on the target desktops (GNOME/GTK and KDE/KWayland at minimum) and record any limitation in the design doc. Verify: manual check that <kbd>Ctrl</kbd>+<kbd>Super</kbd>+<kbd>Space</kbd> logs press and release on each tested desktop; document any desktop where global capture is unavailable.

## 5. Validation

- [ ] 5.1 Run `cargo build` and `cargo test` at the workspace root and confirm both pass. Verify: both commands exit 0.
- [ ] 5.2 Confirm the acceptance criteria from issue #1 are met: the daemon intercepts <kbd>Ctrl</kbd>+<kbd>Super</kbd>+<kbd>Space</kbd> and logs both press and release. Verify: the log output from task 4.2 shows both a press and a release entry for a physical press/release of <kbd>Ctrl</kbd>+<kbd>Super</kbd>+<kbd>Space</kbd>.
