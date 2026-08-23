## Context

The 2026-08-23 spec-vs-implementation review found that the two specs describe the
original design, which was deliberately replaced: the capture key is now
<kbd>Ctrl</kbd>+<kbd>Super</kbd> polled at 15 ms (the
<kbd>Ctrl</kbd>+<kbd>Super</kbd>+<kbd>Space</kbd> hotkey approach "could not get it
work" on the target desktop), and the model file set is the `tdt/` set from
`altunenes/parakeet-rs` (the spec's five files exist nowhere in that repo). The owner
confirmed both design decisions and the flat on-disk layout
(`~/.config/steno/models/parakeet/<basename>`) as the working design. The remaining
work is to fix the genuine bugs in the current tree and amend the specs to the
intentional design.

## Goals / Non-Goals

**Goals:**

- Fix the idle-tick panic in `listen()` and make the capture transition logic a pure,
  OS-free state machine with unit tests.
- Fix the provisioning layout (flat local paths), narrow the required file set to what
  the ParakeetTDT model loads, and add size-based integrity verification with atomic
  placement.
- Surface listener/recorder task failures in `main` as a logged non-zero exit.
- Amend the `capture-key-interception` and `model-provisioning` specs to the
  intentional design, so conformance holds forward.

**Non-Goals:**

- Reconciling grab mode (code uses `HotkeyManager` `.grab()`; the archived capture
  design doc's final form says no grab). Explicitly deferred as a separate backlog
  item; this change does not change grab behavior.
- Rebuilding the `#[ignore]`d provisioning integration test (separate backlog item).
- Content-hash integrity verification (recorded as a follow-up hardening).
- Audio capture, transcription, and injection (the next capability layer).

## Decisions

### Decision: Restore conformance by amending the specs, not reverting the code

The current design is the working, owner-confirmed design. The specs are stale. The
specs are amended in this change (delta specs for both capabilities); the code keeps
the polled <kbd>Ctrl</kbd>+<kbd>Super</kbd> design and the `tdt/` file set. The
rationale for the capture-key change is recorded in the spec so the next reader does
not "fix" it back.

### Decision: Capture key is a polled modifier pair, held in a pure state machine

`CaptureState` is a small pure struct (`active: bool`) with
`handle(ctrl: bool, super_: bool) -> Option<RecorderCommand>`:

- both held and not active ⇒ `Start`, becomes active;
- not both and active ⇒ `Stop`, becomes inactive;
- otherwise ⇒ `None` (no transition).

`KeyListener::listen` polls `active_modifiers()` every 15 ms, feeds the two modifier
bits into the state machine, and sends the command only when a transition occurred —
an idle tick is a no-op (fixing the `Option::unwrap()` panic). Press and release are
logged at this layer (`capture key pressed` / `capture key released`) so the
capture-key spec's logging requirement is met independent of the recorder. The state
machine has no OS access, so its unit tests do not need evdev; the three existing
tests that built a real `HotkeyManager` are replaced.

### Decision: Required file set is the four files the ParakeetTDT model loads

`ParakeetTDT::from_pretrained` (parakeet-rs) requires `vocab.txt` and resolves the
encoder from `encoder-model.onnx` → `encoder.onnx` → `encoder-model.int8.onnx` and the
decoder from `decoder_joint-model.onnx` → `decoder_joint-model.int8.onnx` → …, i.e.
the full-precision names always win when present, so provisioning the int8 variants
would be ~670 MB of dead weight. `nemo128.onnx` is not referenced by the crate.
`encoder-model.onnx.data` is the encoder's external-data file and is required.
Therefore the required set is:

| Repo path (`altunenes/parakeet-rs`) | Local file (flat) | Size (bytes) |
| --- | --- | --- |
| `tdt/decoder_joint-model.onnx` | `decoder_joint-model.onnx` | 72,520,893 |
| `tdt/encoder-model.onnx` | `encoder-model.onnx` | 41,770,866 |
| `tdt/encoder-model.onnx.data` | `encoder-model.onnx.data` | 2,435,420,160 |
| `tdt/vocab.txt` | `vocab.txt` | 93,939 |

Sizes were verified against the HuggingFace repo API on 2026-08-23 and match the
byte-exact files on the owner's machine.

### Decision: Keep `tdt/` in the URL, strip it from the local path

hf-hub's local-dir download writes to `local_dir.join(filename)`, coupling the repo
path and the local name. The daemon therefore downloads each file into a staging
directory (`<config>/steno/models/.parakeet-staging`, same filesystem as the model
directory) — where hf-hub places it at `tdt/<basename>` — verifies the staged file,
then `fs::rename`s it into `parakeet/<basename>`. The rename is atomic (same
filesystem), so the model directory never contains a partial file. The staging
directory is removed (best effort) when the download loop finishes or fails.

### Decision: Integrity is size-based, applied to downloaded files

`REQUIRED_FILES` pins each file's expected byte size. After download, the staged
file's size is checked; on mismatch the staged file is deleted and provisioning
fails (the file is never moved into place, and `ensure_parakeet_model` returns an
error, so the model is not reported ready). Readiness of files already in the model
directory is "exists and non-empty" — the strictest check that does not couple
startups to upstream repo drift (an upstream re-export of the same names would
otherwise trigger a ~2.5 GB re-download every startup). Content-hash pinning is a
follow-up hardening.

### Decision: No config dir, no model dir

`resolve_config_dir` returns `$XDG_CONFIG_HOME` when set, else `$HOME/.config`, else
an error. The `/tmp` fallback is removed: a world-readable model directory contradicts
the architecture goal that model/audio data be accessible only to the logged-in user.
The derivation is testable without touching the process environment (pure function of
the two variables).

### Decision: `main` selects over task completion and the shutdown signal

The listener and recorder are spawned (tracked), and `main` selects over the two task
handles and the SIGINT/SIGTERM futures. If a task ends early — `Err` from the future,
a panic (`JoinError`), or an unexpected clean exit — `main` logs the failure and
returns an error (non-zero exit) instead of waiting for a shutdown signal with dead
capture. The SIGTERM handler registration `expect()` becomes `?`.

## Risks / Trade-offs

- **Upstream drift.** If the repo re-exports the files with different sizes, the
  pinned-size verification fails hard (clear error) instead of silently accepting
  different bytes. Mitigation: update the pinned sizes in one place
  (`REQUIRED_FILES`) when that happens.
- **Polling latency.** The 15 ms poll is the working design's behavior; press/release
  granularity is at most one tick. Accepted (owner-confirmed working on the desktop).
- **Grab mode left inconsistent.** The code grabs; the archived design doc's final
  form does not. Deferred as a separate backlog item; flagged in the archived design
  doc addendum so the disagreement is visible, not silent.
- **Staging directory on crash.** If the daemon is killed mid-download, the staging
  directory may survive; the next run re-downloads into it (hf-hub overwrites) and
  cleans it up on completion. The model directory itself is unaffected.
