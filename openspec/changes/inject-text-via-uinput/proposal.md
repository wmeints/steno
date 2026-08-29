# Proposal: inject-text-via-uinput

## Why

After audio is transcribed, the text has nowhere to go: the daemon can capture and (later) transcribe, but it cannot deliver the result to the focused application. Integrating with each desktop environment or Wayland compositor's text-input protocol would be deep, brittle work. Writing through the kernel's `/dev/uinput` virtual-input device makes the daemon appear as a USB keyboard to every desktop environment at once, with zero compositor integration (issue #3).

## What Changes

- Add a new `uinput` module (`crates/steno-daemon/src/uinput.rs`) exposing a `TextInjector` that opens the system uinput device via the `uinput` crate's `default()` function, registers a virtual keyboard, and types arbitrary text as key events.
- Add text-to-keysym translation for the characters dictation actually produces (letters, digits, punctuation, spaces, newlines, tabs), with shift handling.
- Wire the injector into the daemon: the recorder's stop path (where transcription output will land) sends injected text through a channel to a dedicated injector task, keeping uinput syscalls off the async runtime's hot paths.
- Unsupported characters are skipped with a warning rather than failing the whole injection.
- Document the `/dev/uinput` permission requirement (the daemon must be able to open the device; typically a udev rule or group membership on this workstation).

## Capabilities

### New Capabilities

- `text-injection`: a daemon module that injects text into the system as keyboard input through `/dev/uinput`, covering device lifecycle, character mapping, and delivery semantics.

### Modified Capabilities

- None. `capture-key-interception` and `model-provisioning` requirements are unchanged; only the daemon's internal wiring gains a downstream consumer.

## Impact

- **Code**: new `crates/steno-daemon/src/uinput.rs`; `lib.rs` gains the module; `recorder.rs`/`main.rs` gain the injection call-site and task wiring.
- **Dependencies**: new `uinput` crate (and its `nix`/`bitflags` transitive deps) in `steno-daemon`.
- **System**: requires write access to `/dev/uinput` at runtime; the virtual keyboard appears in the OS input list while the daemon runs.
- **Testing**: character mapping and unsupported-character handling are pure and unit-testable without a device; actual injection is verified manually on the workstation.
