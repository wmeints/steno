# Design: inject-text-via-uinput

## Context

See proposal.md - Why. Current state: `steno-daemon` is a Tokio app (`main.rs`) running a `KeyListener` task (polls Ctrl+Super, sends `RecorderCommand` over an mpsc channel) and a `Recorder` task (state machine toggling `is_recording`; transcription is not yet wired in — stop currently only logs). The issue mandates the `uinput` crate and its `default()` function for device access.

Constraints:
- `/dev/uinput` requires write permission; uinput event writes are blocking file writes.
- AGENTS.md: logic that can run without OS interaction must be unit-testable; cognitive-complexity threshold 10 (denied above that, extract helpers).
- The virtual keyboard is a system-wide side effect: it must not outlive the daemon.

## Goals / Non-Goals

**Goals:**
- A `TextInjector` module with a narrow async-friendly API: `inject(String)` → typed characters.
- Pure, unit-tested char→keysym translation (including shift) with no OS calls.
- Injector runs as its own task; recorder/main hand off text over a channel.
- Clean device teardown on daemon exit.

**Non-Goals:**
- Clipboard-based paste injection (alternative delivery mechanism).
- Non-Latin layouts, compose-key sequences, emoji.
- Typing-speed/inter-key-delay tuning beyond a sane fixed small delay (if needed).
- Actual transcription→injection end-to-end wiring of audio bytes (owned by the capture/transcription changes); this change defines the injection boundary the recorder calls.

## Decisions

### D1: Dedicated injector task behind an mpsc channel

`main.rs` spawns an `Injector` task owning the `uinput::VirtualDevice`, fed by `mpsc::Receiver<String>`. The recorder (later, the transcription callback) sends `tx.send(text)`.

- Rationale: the `VirtualDevice` is not `Sync`/`Send`-friendly for shared async use, and event writes are blocking OS calls; confining the device to one task keeps the runtime's other tasks untouched and serializes keystrokes naturally (satisfies the ordering requirement — one writer, FIFO channel).
- Alternative rejected: a mutex-guarded singleton device called from arbitrary tasks — more sharing hazards, no benefit; and per-call device open/close — slow and leaves focus-change races.

### D2: Device construction via `uinput::default()` + `UInput::build()`

Per the issue: `uinput::default()` to get the system uinput handle, then build a device declaring `KEY_A..KEY_Z`, `KEY_0..KEY_9`, punctuation keysyms, `KEY_LEFTSHIFT`, `KEY_ENTER`, `KEY_TAB`, `KEY_SPACE`. `enable()` opens the device. Construction failure propagates: `main` returns `Err` before spawning tasks → daemon exits non-zero with the error (spec: fail fast on permission denied).

- Device name: `"steno-virtual-keyboard"` (`UInput::with_name`) so it is identifiable in `/proc/bus/input/devices`.

### D3: Pure translation function, OS-free

`fn translate(text: &str) -> Vec<KeyEvent>` (crate-internal, in `uinput.rs`) maps each `char` to press/release `input_event`-shaped values. A static table `CHAR_TO_KEY: Map<char, (Keycode, Shift)>` covers lowercase/digits/space/tab/enter and punctuation; uppercase letters map to the same keycode + `KEY_LEFTSHIFT` press before, release after. Unknown chars are dropped and reported: translation returns `Vec<(KeyEvent, Option<char>)>` or the injector logs skipped chars from the translation result — keep it a pure `Vec` of events plus a skipped-chars list.

- Rationale: 100% unit-testable without `/dev/uinput`; the only OS-touching code is `write(&events)` + device lifecycle.
- Alternative rejected: using the crate's `EmulatedWriter`/rt-features — pulls in more machinery than needed and is less testable.

### D4: Shift via explicit press/release, one flush per character group

For a shifted char, emit `[SHIFT press, KEY press, KEY release, SHIFT release]` in a single `device.write(&events)` call so the kernel sees them atomically in order. No artificial sleep between characters initially; `uinput` events queue in the kernel and X/Wayland handle repeat rate. If real-world drops appear, add a small inter-event delay behind a constant (flagged under Risks).

### D5: Wiring point in the recorder

`Recorder::listen` gains a `Sender<String>` (in addition to today's `Receiver<RecorderCommand>`); on `RecorderCommand::Stop`, the future transcription result will be sent there. For this change, the recorder sends nothing yet (no transcription), but the channel and task are constructed in `main.rs` and the injector drains it — the acceptance-criteria module boundary exists and is exercised by a manual smoke test.

- Smoke test: `steno-daemon` debug hook — a `--inject-smoke TEXT` argv, or unit-level integration test calling the injector loop with a fake device trait. Decision: extract a `Device` trait (`write_events`) so an integration test drives the injector loop with a recording mock; manual `/dev/uinput` verification remains on the workstation.

## Risks / Trade-offs

- **Permission on `/dev/uinput`**: workstation may lack group/udev access; daemon now fails at startup (intentional, spec'd). Mitigation: README/AGENTS note + clear error; user-side udev rule `KERNEL=="uinput", GROUP="input", MODE="0660"` or equivalent.
- **Key drops under load**: the compositor's key-repeat/auto-repeat may swallow fast uinput events on some stacks. Mitigation path: constant inter-event delay (D4) tuned manually; not speculative up front.
- **Focus race**: user releases Ctrl+Super over the capture key, focus unchanged, but a click between stop and injection could retarget text. Accepted: out of scope; mitigation is a compositor-side concern (later changes may add a focus lock or clipboard fallback).
- **Layout assumption**: mapping assumes a US-QWERTY physical layout for symbols. The receiver decodes keysyms to unicode at the compositor, which uses its own layout — on non-US layouts, punctuation may land differently. Accepted for this workstation; documented as a limitation.
- **Phantom device persistence**: a daemon crash leaves the uinput device until the fd closes (kernel auto-destroys on process exit). No mitigation needed; verified in lifecycle scenario.

## Migration Plan

Additive: new module + new task; no data or config migration. Rollback = revert commit. Dependency addition: `uinput` crate (pulls `nix`, `bitflags`, `libc`).

## Open Questions

- Does the recorder's stop path already have a transcription string available in current `main`? (It does not — `Recorder` only logs.) This change deliberately stops at the injection boundary; the transcription→injection handoff lands with the capture/transcription change.
