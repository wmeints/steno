## Why

The specs describe a design that no longer exists — on purpose. The capture key
was deliberately changed from <kbd>Ctrl</kbd>+<kbd>Super</kbd>+<kbd>Space</kbd> to
<kbd>Ctrl</kbd>+<kbd>Super</kbd> because the hotkey-based approach did not work on the
target desktop, and the model file set was deliberately changed to the `tdt/` file set
from `altunenes/parakeet-rs` because the five files the spec names do not exist anywhere
in that repository. Conformance is therefore restored forward, by amending the specs,
not by reverting the code.

Independently of the stale specs, the current tree has genuine bugs that block the
working design:

- `listen()` panics on the first idle tick (`.unwrap()` on `Option::None`), so the
  capture path is dead before the first key is pressed.
- Model provisioning probes `models/tdt/<name>` (a `with_file_name` bug) and writes to
  `parakeet/tdt/<name>`, while the ParakeetTDT model reads flat `parakeet/<name>` — so
  the availability check never passes, every startup re-enters the download branch, and
  a fresh machine would provision a layout the model does not read.
- There is no integrity verification: a truncated or corrupted download is silently
  accepted; a zero-byte file counts as provisioned; the config dir falls back to `/tmp`.
- Listener/recorder task failures (including panics) are invisible: the daemon hangs
  with dead capture and reports nothing; SIGTERM handler registration panics via
  `expect()`.

## What Changes

- **Capture key** (`capture-key-interception`): the spec adopts <kbd>Ctrl</kbd>+<kbd>Super</kbd>
  (both modifiers held, no base key) as the capture key, with the rationale for the
  change recorded. The transition logic moves into a pure, OS-free state machine that is
  unit-tested without evdev access; `KeyListener` becomes a thin evdev adapter. The
  idle-tick panic is fixed (an idle tick is a no-op), and press/release are logged at
  the capture-key layer.
- **Model provisioning** (`model-provisioning`): the spec adopts the `tdt/` file set —
  narrowed to the four files the ParakeetTDT model actually loads — stored flat in
  `~/.config/steno/models/parakeet`. The daemon probes and writes the flat layout (the
  `tdt/` prefix stays in the download URL only), pins the expected byte size of every
  required file, verifies sizes after download and rejects mismatched files (deleted,
  provisioning fails), downloads to a staging path and atomically moves verified files
  into place, treats a file as available only when it exists and is non-empty, and
  fails with an error instead of falling back to `/tmp` when no config dir can be
  derived.
- **Daemon startup**: if the listener or recorder task ends in error, panics, or exits
  early, the daemon logs the failure and exits non-zero instead of hanging; SIGTERM
  handler registration errors propagate instead of panicking.

## Capabilities

### New Capabilities

<!-- none -->

### Modified Capabilities

- `capture-key-interception`: the capture key definition, press detection, and release
  detection change to the <kbd>Ctrl</kbd>+<kbd>Super</kbd> pair (no base key); the
  logging requirement is unchanged but now backed by capture-key-level log lines.
- `model-provisioning`: the model directory location wording (XDG-derived, error when
  not derivable), the fetched file set (four `tdt/` files, flat local layout), missing
  file detection (exists and non-empty), and integrity verification (pinned sizes,
  reject and delete on mismatch, atomic placement) change.
