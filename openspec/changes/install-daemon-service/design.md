# Design: Install Daemon Service

## Context

See proposal.md — Why. Current state:

- `crates/steno-daemon` builds a binary named `steno-daemon` (package-name
  default). Nothing execs the built binary by name today — integration tests
  exercise library entry points in-process — so the rename is low-blast-radius.
- The daemon is long-running, tokio-based, needs the PipeWire socket, the
  session D-Bus bus (for notifications), and `/dev/uinput` (for injection).
- CUDA runtime libs live at `~/.local/lib/steno-cuda` (user-local extraction,
  no system toolkit); the daemon is currently launched as
  `LD_LIBRARY_PATH=$HOME/.local/lib/steno-cuda steno-daemon`.
- Existing capability specs live flat under `openspec/specs/<capability>/`;
  this change adds `daemon-installation` at the same level.

## Goals / Non-Goals

**Goals:**

- User-scoped systemd service (no root-owned system unit, works on any distro
  with systemd user sessions; `After=pipewire.service` resolves against the
  user manager, where PipeWire runs as a user service).
- Install script is plain bash, no clap-like option parsing (AGENTS.md
  preference for std/minimal deps); idempotent; fail-fast with clear step
  names.
- Hardening from issue #6: `DevicePolicy=closed` + `DeviceAllow=/dev/uinput rw`.

**Non-Goals:**

- System-wide (multi-user) installation, packaging formats (.deb/.rpm/AppImage),
  CI release pipelines.
- Uninstall script; auto-update; enabling the `uinput` kernel module
  (assumption: the module is loaded by the distro — see Risks).
- Portal/Flatpak integration.

## Decisions

### D1: Binary rename via `[[bin]]` in Cargo.toml

Add to `crates/steno-daemon/Cargo.toml`:

```toml
[[bin]]
name = "stenod"
path = "src/main.rs"
```

Package name stays `steno-daemon`; only the artifact is `stenod`. Alternative
considered: rename the package — rejected, churns `cargo -p steno-daemon`
invocations, CI workflow, and lib imports (`steno_daemon::…`) for no benefit.

### D2: Unit file shipped as a template, installed verbatim

`packaging/stenod.service`:

```ini
[Unit]
Description=Steno dictation daemon
After=pipewire.service dbus.service

[Service]
ExecStart=%h/.local/bin/stenod
Restart=on-failure
Environment=LD_LIBRARY_PATH=%h/.local/lib/steno-cuda
DevicePolicy=closed
DeviceAllow=/dev/uinput rw
# Keep user D-Bus + PipeWire sockets reachable (they are not device nodes,
# so DevicePolicy=closed does not affect them).

[Install]
WantedBy=default.target
```

Decisions inside:

- `%h` expansion keeps the unit user-independent (one file for all users).
- `Environment=LD_LIBRARY_PATH=` hard-codes the project-standard CUDA path per
  memory/conventions; users elsewhere override via `systemctl --user edit
  stenod` drop-in (standard mechanism, no templating invented here).
- `After=pipewire.service` per issue #6. Added `dbus.service` ordering: the
  notification path needs the user session bus; ordering is cheap insurance.
  (`Requires=` deliberately NOT used: notifications degrade gracefully, the
  daemon shouldn't die if user D-Bus restarts.)
- No `NoNewPrivileges`/`ProtectSystem` in v1: `DevicePolicy` is the hardening
  the issue asked for; full sandbox flags are easy additive follow-ups once
  observed against the real model-loading paths (hf-hub cache under `$HOME`).

### D3: Installer copies files; does not build by default

`scripts/install.sh` flow (as the installing user):

1. Preflight: `command -v systemctl`, user systemd reachable
   (`systemctl --user is-system-running` or best-effort), `~/.local/bin`
   exists (create it).
2. Binary: copy `target/release/stenod` → `~/.local/bin/stenod`. If the binary
   is missing, print `cargo build --release -p steno-daemon` hint and exit 1.
   Rationale: build is slow (CUDA/onnx deps); install ≠ compile. A
   `--build` flag runs the cargo build step first for convenience.
   Also copy `libonnxruntime_providers_shared.so` and
   `libonnxruntime_providers_cuda.so` from `target/release/` beside the
   binary (found during E2E: ONNX Runtime dlopen's provider libraries via
   `$ORIGIN` — the binary's own directory — and silently falls back to CPU
   when they are absent; `LD_LIBRARY_PATH` alone does not help, the files
   must sit next to `stenod`).
3. Unit: copy `packaging/stenod.service` → `~/.config/systemd/user/`,
   `systemctl --user daemon-reload`, `systemctl --user enable --now stenod`.
4. Udev (sudo): install `packaging/99-uinput.rules` →
   `/etc/udev/rules.d/99-uinput.rules` (via `sudo tee`, single canonical copy
   — re-run overwrites the same path, idempotent);
   `getent group uinput || sudo groupadd -r uinput`;
   `id -nG $USER | grep -qw uinput || sudo usermod -aG uinput $USER`;
   `sudo udevadm control --reload-rules && sudo udevadm trigger /dev/uinput`
   (the `static_node=` option covers coldplug at boot; the `--since`-style
   `udevadm trigger` re-applies group/mode to the existing node).
5. Post-check: if the current process is not yet in the `uinput` group
   (newly added), print "log out and back in, then verify with
   `systemctl --user status stenod`" and exit 0 — the enable/restart is
   already queued; the service will gain access after re-login.
   If sudo fails at step 4, exit non-zero naming the step and what was already
   done (spec: "sudo unavailable or refused").

Alternatives: systemd-tmpfiles or a Makefile — rejected; issue asked for bash.

### D4: Rule file content exactly as issue #6

`packaging/99-uinput.rules`:

```
KERNEL=="uinput", SUBSYSTEM=="misc", MODE="0660", GROUP="uinput", OPTIONS+="static_node=uinput"
```

`OPTIONS+="static_node=uinput"` matters: it makes udev create `/dev/uinput`
with these attributes when the module loads, covering the boot-time case.

### D5: Capability spec placement

New flat capability `specs/daemon-installation/spec.md` matching the repo's
existing flat layout (`text-injection`, `capture-key-interception`, …).

## Risks / Trade-offs

- **`uinput` kernel module not loaded** on some distros: the rule only sets
  permissions, it does not load the module. Mitigation: installer checks
  `/dev/uinput` existence and, if absent, prints the
  `echo uinput | sudo tee /etc/modules-load.d/uinput.conf` remedy rather than
  silently succeeding. (Doing it automatically via sudo was considered; kept
  as printed guidance to limit surprise.)
- **Group grant is broad**: mode `0660` group `uinput` lets any user in that
  group inject keystrokes as anyone in their own session. Accepted — this is
  the mechanism issue #6 specifies; hardening alternative (systemd-owned
  device via `DeviceAllow` alone) was investigated: `DevicePolicy=closed` +
  `DeviceAllow` already lets *the service* write `/dev/uinput` even without
  the udev rule, but the rule is still required to run the daemon by hand and
  for the uinput integration tests; keep both.
- **`LD_LIBRARY_PATH` pinning** to `~/.local/lib/steno-cuda` is wrong for
  non-standard layouts; drop-in override documented in the unit and README
  touch.
- **Enable --now before group re-login** leaves the service crash-looping
  (`Restart=on-failure`) until the user re-logs in. Mitigated by step-5
  message; alternative (skip `--now`) was rejected: on machines where the user
  is already in `uinput`, immediate start is the desired UX.
- **sudo in the middle** makes the script two-phase; ordering privileged steps
  last minimizes partial-install surface.

## Migration

None needed: existing manual-launch users keep working (`stenod` runnable by
hand); the only visible change is the binary filename, currently referenced
nowhere in scripts/docs beyond the README TODO.
