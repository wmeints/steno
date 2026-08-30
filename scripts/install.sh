#!/usr/bin/env bash
# Install the Steno dictation daemon:
#   - ~/.local/bin/stenod                (daemon binary)
#   - ~/.config/systemd/user/stenod.service (user service, enabled)
#   - /etc/udev/rules.d/99-uinput.rules  (uinput group access, via sudo)
#
# Usage: scripts/install.sh [--build]
#   --build  run `cargo build --release -p steno-daemon` first
#
# Safe to re-run: every step overwrites its own target.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DST="$HOME/.local/bin/stenod"
UNIT_DST="$HOME/.config/systemd/user/stenod.service"
RULE_DST="/etc/udev/rules.d/99-uinput.rules"

# Works from two layouts: the repository (target/release + packaging/ dirs)
# and a flat release package where stenod, the provider .so files, the unit,
# and the rule sit beside this script.
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
pick() { # first existing candidate
  local c
  for c in "$@"; do
    [[ -e "$c" ]] && { echo "$c"; return 0; }
  done
  return 1
}
BIN_SRC="$(pick "$SCRIPT_DIR/stenod" "$REPO_ROOT/target/release/stenod")" || BIN_SRC="$REPO_ROOT/target/release/stenod"
ORT_DIR="$(dirname "$BIN_SRC")"
UNIT_SRC="$(pick "$SCRIPT_DIR/stenod.service" "$REPO_ROOT/packaging/stenod.service")" || UNIT_SRC="$REPO_ROOT/packaging/stenod.service"
RULE_SRC="$(pick "$SCRIPT_DIR/99-uinput.rules" "$REPO_ROOT/packaging/99-uinput.rules")" || RULE_SRC="$REPO_ROOT/packaging/99-uinput.rules"

usage() {
  sed -n '2,9p' "${BASH_SOURCE[0]}"
}

fail() {
  echo "install.sh: ERROR: $1" >&2
  exit 1
}

step() {
  echo "==> $1"
}

DO_BUILD=0
for arg in "$@"; do
  case "$arg" in
    --build) DO_BUILD=1 ;;
    -h | --help)
      usage
      exit 0
      ;;
    *) fail "unknown argument: $arg (see --help)" ;;
  esac
done

# --- Step 1: preflight -------------------------------------------------------
step "preflight"
command -v systemctl >/dev/null 2>&1 ||
  fail "preflight: systemctl not found; this installer requires systemd"
if ! systemctl --user is-system-running >/dev/null 2>&1 &&
  ! systemctl --user show-environment >/dev/null 2>&1; then
  fail "preflight: cannot reach the user systemd manager (is XDG_RUNTIME_DIR set inside a graphical session?)"
fi
mkdir -p "$HOME/.local/bin" "$HOME/.config/systemd/user"

# --- Step 2: binary -----------------------------------------------------------
if [[ "$DO_BUILD" == "1" ]]; then
  step "cargo build --release -p steno-daemon"
  (cd "$REPO_ROOT" && cargo build --release -p steno-daemon)
fi
[[ -x "$BIN_SRC" ]] ||
  fail "install binary: $BIN_SRC not found — run 'cargo build --release -p steno-daemon' first, or re-run with --build"
step "install binary -> $BIN_DST"
# Stop first so the service never runs a half-replaced binary; rm+install also
# avoids ETXTBSY when the old process is still up.
systemctl --user stop stenod.service 2>/dev/null || true
rm -f "$BIN_DST"
install -m 0755 "$BIN_SRC" "$BIN_DST"
# ONNX Runtime dlopen's its execution providers from the binary's own
# directory ($ORIGIN), so the provider libraries must sit beside stenod.
# (install follows the cargo-build symlinks into the ort download cache.)
for lib in libonnxruntime_providers_shared.so libonnxruntime_providers_cuda.so; do
  if [[ -e "$ORT_DIR/$lib" ]]; then
    install -m 0644 "$ORT_DIR/$lib" "$HOME/.local/bin/$lib"
  else
    echo "WARNING: $ORT_DIR/$lib not found — transcription will fall back to CPU" >&2
  fi
done

# --- Step 3: systemd user service ---------------------------------------------
[[ -f "$UNIT_SRC" ]] || fail "install unit: $UNIT_SRC missing from repository"
step "install unit -> $UNIT_DST"
install -m 0644 "$UNIT_SRC" "$UNIT_DST"
step "systemctl --user daemon-reload && enable --now stenod"
systemctl --user daemon-reload
systemctl --user enable --now stenod.service

# --- Step 4: udev rule + uinput group (privileged) ------------------------------
step "udev rule + uinput group (requires sudo)"
command -v sudo >/dev/null 2>&1 ||
  fail "udev step: sudo not found — binary and service are installed, but the udev rule and group were NOT"
sudo -v ||
  fail "udev step: sudo authorization refused — binary and service are installed, but the udev rule and group were NOT"

install_udev_rule() {
  [[ -f "$RULE_SRC" ]] || fail "udev step: $RULE_SRC missing from repository"
  sudo install -m 0644 -o root -g root "$RULE_SRC" "$RULE_DST"
}
install_udev_rule ||
  fail "udev step: could not write $RULE_DST — binary and service are installed; rule and group were NOT"

if ! getent group uinput >/dev/null 2>&1; then
  step "creating system group 'uinput'"
  sudo groupadd -r uinput ||
    fail "udev step: groupadd -r uinput failed — binary/service/rule are installed; group membership was NOT"
fi

NEEDS_RELOGIN=0
# The group *database* decides whether usermod must run (id -nG shows only
# groups from this login session, not a group added after login).
uinput_has_user() {
  [[ "$(id -gn)" == "uinput" ]] && return 0
  getent group uinput | cut -d: -f4 | tr ',' '\n' | grep -qx "$USER"
}
if uinput_has_user; then
  step "user '$USER' is already a member of group 'uinput'"
else
  step "adding user '$USER' to group 'uinput'"
  sudo usermod -aG uinput "$USER" ||
    fail "udev step: usermod -aG uinput $USER failed — rule is installed; group membership was NOT"
  NEEDS_RELOGIN=1
fi

step "reloading udev rules"
sudo udevadm control --reload-rules ||
  fail "udev step: udevadm control --reload-rules failed"
sudo udevadm trigger --subsystem-match=misc --sysname-match=uinput || true

# --- Step 5: post-checks --------------------------------------------------------
if [[ ! -e /dev/uinput ]]; then
  echo "WARNING: /dev/uinput does not exist; the uinput kernel module is probably not loaded." >&2
  echo "         Load it persistently with:" >&2
  echo "             echo uinput | sudo tee /etc/modules-load.d/uinput.conf" >&2
  echo "         then reboot (or 'sudo modprobe uinput' for this boot)." >&2
fi

if [[ "$NEEDS_RELOGIN" == "1" ]]; then
  echo "NOTE: group membership takes effect after the next login." >&2
  echo "      The service is enabled and will run with uinput access after you" >&2
  echo "      log out and back in. Verify afterwards with:" >&2
  echo "          systemctl --user status stenod" >&2
fi

step "done"
echo "Installed: $BIN_DST"
echo "Service:   stenod.service (user, enabled)$([[ $NEEDS_RELOGIN == 1 ]] && echo ' — re-login required for device access')"
echo "Verify:    systemctl --user status stenod"
