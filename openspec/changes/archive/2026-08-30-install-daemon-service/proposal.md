# Install Daemon Service

## Why

The daemon must be started by hand from a terminal, and each session needs the
`LD_LIBRARY_PATH` wrapper plus manual `/dev/uinput` permission fixes. GitHub
issue #6 asks for the daemon to run as a background service managed by the
user's systemd instance, with a udev rule granting persistent uinput access,
and a single install script wiring everything up.

## What Changes

- Rename the built executable from `steno-daemon` to `stenod` (crate/package
  name stays `steno-daemon`; only the `[[bin]]` name changes). Downstream
  references that exec the built binary by name follow the rename.
- Add a systemd **user** unit that runs `stenod` under the user's manager,
  restarts on failure, starts after PipeWire, and is hardened with
  `DevicePolicy=closed` + `DeviceAllow=/dev/uinput rw`.
- Add a udev rule (`99-uinput.rules`) granting the `uinput` group rw access to
  `/dev/uinput`.
- Add a bash install script (`scripts/install.sh`) that installs the binary to
  `~/.local/bin/stenod`, installs and enables the user unit, and — via `sudo` —
  writes the udev rule, creates the `uinput` group if missing, adds the current
  user to it, and reloads udev.
- Uninstall counterpart is **out of scope** (issue #6 lists installation only).

## Capabilities

### New Capabilities

- `daemon-installation`: how the daemon binary, its systemd user service, and
  its persistent uinput device permission are installed and configured on a
  target Linux system.

### Modified Capabilities

- None. The `text-injection` requirement that the daemon fail fast when
  `/dev/uinput` is inaccessible is unchanged; this install only makes that
  access persistent through the installed udev rule and `uinput` group
  membership, and the binary rename alters no specified behavior.

## Impact

- `crates/steno-daemon/Cargo.toml`: add `[[bin]] name = "stenod"`.
- New files: `packaging/stenod.service`, `packaging/99-uinput.rules`,
  `scripts/install.sh`.
- Any integration test or doc that execs the built binary by name uses `stenod`.
- Runtime: CUDA builds still need `~/.local/lib/steno-cuda`; the unit sets
  `Environment=` so the service finds those libraries.
- Root privileges required once, at install time, for the udev rule (sudo).
- No public API or wire-format changes.
