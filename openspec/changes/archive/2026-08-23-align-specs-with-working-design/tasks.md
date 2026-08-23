## 1. Capture key: pure state machine, panic fix (P0-1, P1-7)

- [x] 1.1 Add a pure `CaptureState` struct (no OS access) to `listener.rs` with
  `handle(ctrl: bool, super_: bool) -> Option<RecorderCommand>`: both held and not
  active ⇒ `Start`; not both and active ⇒ `Stop`; otherwise `None`.
  Verify: unit tests cover press from idle, press while a modifier is already held,
  single modifier does not activate, release on either modifier dropped, release both
  together, repeated cycles in order, and idle/unchanged ticks emit nothing.
- [x] 1.2 Rewrite `KeyListener::listen` to feed the polled modifier bits into
  `CaptureState` and send only `Some(cmd)` transitions (an idle tick is a no-op — no
  `.unwrap()` on `None`); log `capture key pressed` / `capture key released` on
  transition. Verify: `cargo build`; a running listener task survives idle ticks
  (it previously panicked at `listener.rs:44` within ~0.4 s of start).
- [x] 1.3 Remove the three OS-coupled tests that built a real `HotkeyManager`; the
  pure state-machine tests replace them.
  Verify: `cargo test -p steno-daemon` passes without evdev keyboard access.

## 2. Model provisioning: flat layout, file set, integrity (P0-2, P1-4, P1-5)

- [x] 2.1 Narrow `REQUIRED_FILES` to the four files the ParakeetTDT model loads
  (`decoder_joint-model.onnx`, `encoder-model.onnx`, `encoder-model.onnx.data`,
  `vocab.txt`), each as (repo path `tdt/<basename>`, pinned expected size) — sizes
  verified against the HF repo API. Drop the int8 variants and `nemo128.onnx`
  (loader fallbacks that are shadowed / not referenced).
  Verify: `cargo build`; the set matches the ParakeetTDT loader's file resolution.
- [x] 2.2 Make the local layout flat: probe and place files at
  `<model dir>/<basename>` (strip `tdt/` from local paths, keep it in the download
  URL). Readiness = every required file exists and is non-empty.
  Verify: unit tests on a temp dir — all present ⇒ available; one missing ⇒ not;
  one zero-byte ⇒ not.
- [x] 2.3 Download each file into a staging directory (hf-hub writes
  `staging/tdt/<basename>`), verify the staged byte count against the pinned size,
  then rename into `<model dir>/<basename>`; on mismatch or download failure, delete
  the staged file and return an error; clean up the staging directory when the loop
  finishes or fails.
  Verify: unit test — a wrong-size staged file is rejected (error, file deleted,
  destination not created); a right-size staged file is moved into place.
- [x] 2.4 Replace the `/tmp` config-dir fallback with an error when neither
  `$XDG_CONFIG_HOME` nor `$HOME` is set; keep XDG derivation.
  Verify: unit test — XDG preferred, `$HOME/.config` fallback, neither ⇒ error.

## 3. Daemon main: surface task failures (P1-6)

- [x] 3.1 Select in `main` over the shutdown signal and the listener/recorder task
  handles; on task `Err`, panic (`JoinError`), or unexpected clean exit, log the
  failure and exit non-zero. Replace the SIGTERM handler `expect()` with `?`
  propagation.
  Verify: `cargo build`; a dying listener task ends the daemon with a logged error
  and non-zero status (e.g. no evdev input access on a headless host).

## 4. Specs and docs (P0-3)

- [x] 4.1 Amend the `model-provisioning` spec: XDG-derived location with error
  fallback, flat layout, the four-file set with rationale, size-based integrity with
  atomic placement, exists-and-non-empty readiness.
- [x] 4.2 Amend the `capture-key-interception` spec: capture key is
  <kbd>Ctrl</kbd>+<kbd>Super</kbd> with no base key, with the rationale for the
  change recorded.
- [x] 4.3 Add a "Post-archive update" addendum to both archived design docs
  recording the intentional redesign; flag the grab-mode disagreement as an open
  item in the capture design doc.
  Verify: `openspec validate` passes; archiving the change merges the deltas into
  the live specs.

## 5. Verification

- [x] 5.1 `cargo build`, `cargo test`, and `cargo clippy --all-targets` pass with no
  new warnings.
- [x] 5.2 Smoke run: the daemon starts, stays alive indefinitely across idle ticks
  (previously it panicked at `listener.rs:44` ~0.4 s after start), and shuts down
  cleanly with exit 0 on SIGTERM. Press/release log ordering is covered by the
  `CaptureState` unit tests plus the unchanged, owner-verified kbd-global polling
  path (an existing library behavior not re-verified here with a synthetic device).
-  Verify: a dying listener task (injected panic) ends the daemon with a logged
  `ERROR` and exit code 1.
- [x] 5.3 Fix all remaining clippy warnings (needless borrow in the new model test,
  two `bool_assert_comparison` in the recorder tests). CI already enforces
  `clippy -- -D warnings`; confirm the full check is clean.
  Verify: `cargo clippy --all-targets --all-features -- -D warnings` exits 0.
