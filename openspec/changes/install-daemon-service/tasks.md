# Tasks: Install Daemon Service

## 1. Rename the binary to stenod

- [x] 1.1 Add `[[bin]] name = "stenod", path = "src/main.rs"` to
  `crates/steno-daemon/Cargo.toml` (package name unchanged).
  Verify: `cargo build -p steno-daemon` produces
  `target/debug/stenod` and no `target/debug/steno-daemon`.
- [x] 1.2 Update any references to the old binary filename in docs/README if
  present (grep for `target/*/steno-daemon` and exec-by-name usage; tests use
  the lib target so likely none).
  Verify: `grep -rn 'bin.*steno-daemon\|target.*steno-daemon'` shows no stale
  executable references.

## 2. Packaging files

- [x] 2.1 Create `packaging/stenod.service` exactly per design D2: user unit,
  `ExecStart=%h/.local/bin/stenod`, `Restart=on-failure`,
  `After=pipewire.service dbus.service`,
  `Environment=LD_LIBRARY_PATH=%h/.local/lib/steno-cuda`,
  `DevicePolicy=closed`, `DeviceAllow=/dev/uinput rw`,
  `WantedBy=default.target`.
  Verify: `systemd-analyze --user verify`-style syntax check — copy to
  `~/.config/systemd/user/`, `systemctl --user daemon-reload`,
  `systemctl --user cat stenod` round-trips the file (then disable/clean up).
- [x] 2.2 Create `packaging/99-uinput.rules` with the issue #6 rule line
  (`KERNEL=="uinput", SUBSYSTEM=="misc", MODE="0660", GROUP="uinput",
  OPTIONS+="static_node=uinput"`).
  Verify: `udevadm verify` (if available on the host) reports no problems.

## 3. Installer script

- [x] 3.1 Write `scripts/install.sh` (bash, `set -euo pipefail`) implementing
  design D3 order: preflight → binary copy (`--build` flag runs
  `cargo build --release -p steno-daemon` first) → unit copy +
  `daemon-reload` + `enable --now` → sudo udev block (rule via `tee`,
  `groupadd -r uinput` if missing, `usermod -aG uinput` if missing,
  `udevadm control --reload-rules && udevadm trigger`). Every failing step
  prints its name and exits non-zero. Verify: `bash -n scripts/install.sh`;
  shellcheck if available.
- [x] 3.2 Add post-check messaging per D3 step 5: warn on missing group
  membership ("log out and back in"), warn + print remedy when `/dev/uinput`
  does not exist (modules-load.d hint), skip `usermod` when the user is
  already in `uinput`.
  Verify: dry inspection — run the group/existence checks standalone on this
  machine and confirm correct branch selection.
- [x] 3.3 Idempotency: re-run installs the same single rule copy, overwrites
  binary, succeeds. Verify: run script twice (see 4.1) and
  `ls /etc/udev/rules.d/99-uinput.rules` once; second run exits 0.

## 4. End-to-end verification (manual — needs physical desktop session)

- [x] 4.1 On the workstation: `cargo build --release -p steno-daemon`, then
  `scripts/install.sh`. Human checks: `~/.local/bin/stenod` exists;
  `systemctl --user status stenod` active (or pending re-login warning);
  `/etc/udev/rules.d/99-uinput.rules` present; `getent group uinput` lists
  the user.
- [x] 4.2 After re-login: `systemctl --user status stenod` shows active;
  daemon log contains the CUDA provider registration line (per project
  convention: verify the log, don't assume); `ls -l /dev/uinput` shows
  `crw-rw---- 1 root uinput`.
- [x] 4.3 Dictation smoke: Ctrl+Super gesture captures audio, transcribed
  text types into the focused app from the service-run daemon (no terminal
  launch). Sandbox check: under the service, open of another device node
  (e.g. `/dev/input/event0`) is denied while `/dev/uinput` works.
- [x] 4.4 Reboot persistence: after reboot, service auto-runs (login target)
  and injection still works.

## 5. Cleanup / docs

- [x] 5.1 Update README "Getting started" TODO with a short install section:
  build command, `scripts/install.sh`, re-login note, drop-in override hint
  for non-standard CUDA paths. Verify: rendered markdown matches actual
  script behavior.
- [x] 5.2 Run `cargo clippy --all-targets --all-features -- -D warnings` and
  `cargo fmt --check`. Verify: exit 0.
