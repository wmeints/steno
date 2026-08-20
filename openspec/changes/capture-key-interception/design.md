## Context

`steno-daemon` is a long-running Rust process (`crates/steno-daemon`) that, per the architecture docs, owns audio capture, transcription, and input injection. Its `main.rs` currently only prints a greeting. To record, the daemon must first observe the keyboard: the user holds a capture hotkey to start recording and releases it to stop.

Global key interception on Linux is not a single well-supported mechanism across desktops. The project must work across major desktop environments (GNOME/GTK, KDE/KWayland, COSMIC, Wayland). The architecture already relies on PipeWire (audio), `/dev/uinput` (input injection), and D-Bus (notifications); a keyboard-capture dependency is a new external dependency the daemon must adopt.

## Goals / Non-Goals

**Goals:**
- Detect the <kbd>Ctrl</kbd>+<kbd>Super</kbd> capture key as a press (inactive→active) and release (active→inactive).
- Log a distinct entry on press and on release so an operator can verify interception from the daemon's log output.
- Detect press even when one modifier is already held before the other, and detect release as soon as either modifier is dropped.

**Non-Goals:**
- Starting or stopping audio recording (PipeWire).
- Triggering transcription (Parakeet) or injecting text (`/dev/uinput`).
- Making the capture key configurable.
- Broadcasting D-Bus notifications for press/release (a later capability).

## Decisions

### Decision: Represent the capture key as a simultaneous-hold state, not a single event

The capture key is "Ctrl held AND Super held." Track the pressed state of each modifier independently, and derive an `active` boolean (`ctrl && super`). Compare it against the previous `active` value to emit exactly one press on the false→true transition and one release on the true→false transition. This satisfies the "one modifier already held" press scenario and the "release one modifier" release scenario without extra special-casing.

Rationale: The spec requires a press when both become held regardless of order, and a release the moment either drops. A derived boolean from the two modifier states is the minimal model that produces exactly one press and one release per cycle, in order, for repeated cycles.

### Decision: Use a global key-capture dependency (candidate: `evdev`/`uinput`-adjacent or `wayland`/`udev` global grab)

Global key interception must work across desktops. The design adopts a single global keyboard-event source for the daemon rather than per-application hooks, because the capture key must fire regardless of which window is focused. The exact binding is chosen at implementation time to satisfy "works on most widely used desktop environments and the terminal." A reasonable starting point is a crate that registers a global hotkey/keyboard listener (e.g. `evdev` grabbing the keyboard, or a Wayland global key source), with the concrete choice validated during implementation.

Rationale: The capture must be global, so a per-window or accessibility-API approach is unsuitable. A single source keeps the state machine above in one place. The choice is left open in the spec (behavior only) and pinned here as an implementation decision with a validation step.

### Decision: Log via the daemon's standard logging channel

Press and release are logged through a standard Rust logging mechanism so the output is greppable and distinguishable (e.g. a `capture_key` module with `pressed` / `released` markers). The log content is the externally observable signal the spec's logging requirement checks.

### Decision: Isolate the key source behind a trait for testing

Expose the keyboard event source behind a small trait so the state machine (active derivation, transition detection, logging) can be tested with a synthetic event stream, independent of the real OS capture binding.

## Risks / Trade-offs

- **Desktop/Wayland compatibility**: A global key grab may require elevated permissions or may not behave identically across Wayland sessions. Mitigation: validate against the target desktops (GNOME/GTK, KDE/KWayland, COSMIC) during implementation; if a single binding cannot cover all, prefer a source that works on the most common and document the limitation.
- **Hotkey conflict with the desktop environment**: <kbd>Ctrl</kbd>+<kbd>Super</kbd> may be claimed by a desktop environment for its own action, so the daemon's grab could be starved. Mitigation: the spec fixes the combination; conflict handling (configurability, override) is explicitly a non-goal here and deferred.
- **Stuck-state edge cases**: If a modifier release event is missed, the daemon could think the key is still held. Mitigation: treat each raw modifier state update as authoritative and recompute `active`; do not rely on paired press/release bookkeeping.
- **Permission requirement**: A global keyboard grab may need the user to be in an input group or run with elevated rights. This is surfaced in tasks as a validation step rather than hidden.
