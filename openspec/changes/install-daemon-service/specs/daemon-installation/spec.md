# daemon-installation Delta Spec

## Purpose

Defines how the Steno daemon becomes a persistent, self-starting part of a
user's Linux session: where the binary is installed, how it is run as a user
systemd service, and how access to the uinput device is granted durably so
dictation works from login without manual setup.

## ADDED Requirements

### Requirement: Installed daemon binary

The installation SHALL place the daemon executable at `~/.local/bin/stenod` and
it SHALL be runnable from a login shell without arguments. The binary name the
service uses MUST match the name the build produces.

#### Scenario: Binary present after install

- **WHEN** installation has completed for a user
- **THEN** `~/.local/bin/stenod` exists, is executable, and starting it by hand
  runs the daemon (it acquires its devices and logs startup)

#### Scenario: Stale previous install is replaced

- **WHEN** installation runs and `~/.local/bin/stenod` already exists from a
  previous install
- **THEN** the file is overwritten with the freshly built binary and the
  install still succeeds

### Requirement: User-scoped systemd service

The installation SHALL register a systemd **user** service named `stenod`
(unit file `stenod.service` under the user unit lookup path) that runs the
installed binary, restarts it on failure, and starts only after the user's
PipeWire service is available. Enabling the service SHALL make it start on
login without further user action.

#### Scenario: Service starts the daemon

- **WHEN** the user's systemd manager starts `stenod.service`
- **THEN** a daemon process is running from `~/.local/bin/stenod` and reports
  healthy status via `systemctl --user status stenod`

#### Scenario: Daemon crash is restarted

- **WHEN** the daemon process exits non-zero
- **THEN** systemd restarts it automatically

#### Scenario: Service enabled across login

- **WHEN** installation has completed and the user logs out and back in
- **THEN** `stenod.service` is running again with no manual start

#### Scenario: PipeWire ordering

- **WHEN** the user session starts with PipeWire not yet up
- **THEN** the daemon is not started before the user PipeWire service has
  started

### Requirement: Service hardening

The unit SHALL restrict device access to exactly what the daemon needs:
`DevicePolicy=closed` with `DeviceAllow=/dev/uinput rw`, so the daemon can
open the virtual-input device but no other device nodes. The user socket for
D-Bus notifications and the PipeWire socket MUST keep working under this
policy without extra device allowances.

#### Scenario: uinput allowed

- **WHEN** the daemon runs under the hardened unit
- **THEN** opening `/dev/uinput` succeeds and text injection works

#### Scenario: unrelated devices denied

- **WHEN** the daemon runs under the hardened unit and attempts to open a
  device node other than `/dev/uinput`
- **THEN** the open is denied by the service sandbox

### Requirement: Durable uinput device access via udev

The installation SHALL deploy the udev rule `99-uinput.rules` to
`/etc/udev/rules.d/` so that `/dev/uinput` is group-owned by `uinput` with
mode `0660`. The install SHALL create the `uinput` group if it does not exist
and SHALL add the installing user to it. Rule deployment and group setup
require root and SHALL be performed through `sudo`.

#### Scenario: Rule active after reboot

- **WHEN** installation has completed and the machine reboots
- **THEN** `/dev/uinput` exists with group `uinput` and mode `0660`, and a user
  in the `uinput` group can open it for writing

#### Scenario: Group membership missing

- **WHEN** the installing user was added to the `uinput` group by this install
  but has not logged in again
- **THEN** the installer informs the user that a re-login (or reboot) is needed
  before the daemon service can access the device

#### Scenario: sudo unavailable or refused

- **WHEN** the privileged step cannot run (no sudo, wrong password, or user
  declined)
- **THEN** the installer exits non-zero with a message naming the step that
  failed and what was already installed

### Requirement: Runtime library environment for the service

The service SHALL start the daemon with the environment it needs to find the
locally-installed CUDA runtime libraries (`~/.local/lib/steno-cuda` on this
project's standard setup), so transcription uses the CUDA execution provider
without the user exporting anything by hand. Users whose libraries live
elsewhere MUST be able to override the path via a systemd user drop-in
(`systemctl --user edit stenod`).

#### Scenario: CUDA provider loads under the service

- **WHEN** the daemon starts via `stenod.service` on a machine with the
  standard `~/.local/lib/steno-cuda` layout
- **THEN** the daemon log shows the CUDA execution provider registered and the
  service keeps running

#### Scenario: Library path overridden by drop-in

- **WHEN** a user adds a drop-in that overrides the library-path environment
  variable and restarts the service
- **THEN** the daemon starts using the overridden path and the unit file itself
  is unmodified

### Requirement: One-command idempotent installer

The installation SHALL be achievable by running a single bash script
(`scripts/install.sh`) as the installing user, with no prompts beyond `sudo`
authorization. Re-running the script SHALL leave the system in the same
end state (binary replaced, unit re-registered, rule present once) and SHALL
succeed. If any step fails, the script MUST stop with a non-zero exit code and
identify the failed step.

#### Scenario: Fresh install end to end

- **WHEN** a user on a supported Linux desktop runs `scripts/install.sh` once
  and re-logs in
- **THEN** `stenod.service` is enabled and running, and a dictation capture
  injects text into the focused application without any manual permission
  fixing

#### Scenario: Re-run after successful install

- **WHEN** the script is run a second time
- **THEN** it exits zero, the service is running, and `/etc/udev/rules.d/`
  contains exactly one copy of the rule
