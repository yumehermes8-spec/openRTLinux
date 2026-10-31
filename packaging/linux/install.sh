#!/usr/bin/env bash
# Install rtlinux for the current user: the binary, the icons, the menu entry.
#
#   bash packaging/linux/install.sh            # debug build (faster to rebuild)
#   bash packaging/linux/install.sh --release  # optimised build
#
# Nothing needs root. Icons go into the user's hicolor theme so the menu entry
# and any launcher can find "rtlinux" by name.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"

# cargo lives in ~/.cargo/bin and a non-login shell (ssh, cron) does not have it
if [[ -x "${HOME}/.cargo/bin/cargo" ]]; then
  PATH="${HOME}/.cargo/bin:${PATH}"
  export PATH
fi

PROFILE="debug"
CARGO_FLAGS=()
BUILD=1
EXTENSION=1
ENABLE_SERVICE=1
ENABLE_TRAY=0
for arg in "$@"; do
  case "${arg}" in
    --release) PROFILE="release"; CARGO_FLAGS+=(--release) ;;
    --no-build) BUILD=0 ;;
    --no-extension) EXTENSION=0 ;;
    --no-service) ENABLE_SERVICE=0 ;;
    --with-tray) ENABLE_TRAY=1 ;;
  esac
done

BIN_DIR="${HOME}/.local/bin"
ICON_ROOT="${HOME}/.local/share/icons/hicolor"
APP_DIR="${HOME}/.local/share/applications"

if [[ "${BUILD}" == "1" ]]; then
  echo "== building rtlinux (${PROFILE}) =="
  cd "${REPO}"
  cargo build --features linuxos --bin rtlinux "${CARGO_FLAGS[@]}"
else
  echo "== skipping the build (--no-build) =="
fi

echo "== installing =="
install -Dm755 "${REPO}/target/${PROFILE}/rtlinux" "${BIN_DIR}/rtlinux"
for size in 16 22 24 32 48 64 128 256 512; do
  src="${HERE}/hicolor/${size}x${size}/apps/rtlinux.png"
  [[ -f "${src}" ]] || continue
  install -Dm644 "${src}" "${ICON_ROOT}/${size}x${size}/apps/rtlinux.png"
done
[[ -f "${HERE}/hicolor/scalable/apps/rtlinux.svg" ]] && \
  install -Dm644 "${HERE}/hicolor/scalable/apps/rtlinux.svg" \
    "${ICON_ROOT}/scalable/apps/rtlinux.svg"
[[ -f "${HERE}/rtlinux.svg" ]] && \
  install -Dm644 "${HERE}/rtlinux.svg" "${ICON_ROOT}/scalable/apps/rtlinux.svg"
install -Dm644 "${HERE}/rtlinux.desktop" "${APP_DIR}/rtlinux.desktop"

# --- tray + settings window (Python/GTK3, talks to the daemon over its socket) ---
install -Dm755 "${REPO}/gui/rtlinux_tray.py" "${BIN_DIR}/rtlinux-tray"
install -Dm644 "${REPO}/gui/rtlinux-tray.desktop" "${APP_DIR}/rtlinux-tray.desktop"

# GNOME has no tray of its own: icons need the AppIndicator host extension.
# Enabling it is a per-user setting, and it is what makes the tray visible.
if [[ "${EXTENSION}" == "1" ]] && command -v gnome-extensions >/dev/null; then
  if gnome-extensions list 2>/dev/null | grep -q ubuntu-appindicators; then
    gnome-extensions enable ubuntu-appindicators@ubuntu.com 2>/dev/null || true
    echo "tray host: ubuntu-appindicators@ubuntu.com enabled (GNOME needs it)"
  else
    echo "tray host: ubuntu-appindicators is not installed - the tray icon will not show"
  fi
fi

# --- systemd user units: a daemon that types must be supervised, not orphaned ---
UNIT_DIR="${HOME}/.config/systemd/user"
install -Dm644 "${HERE}/rtlinux.service" "${UNIT_DIR}/rtlinux.service"
install -Dm644 "${HERE}/rtlinux-tray.service" "${UNIT_DIR}/rtlinux-tray.service"
if command -v systemctl >/dev/null && systemctl --user show-environment >/dev/null 2>&1; then
  systemctl --user daemon-reload || true
  if [[ "${ENABLE_SERVICE}" == "1" ]]; then
    systemctl --user enable --now rtlinux.service >/dev/null 2>&1 || true
    echo "service : rtlinux.service enabled (starts with the session)"
    if [[ "${ENABLE_TRAY}" == "1" ]]; then
      systemctl --user enable --now rtlinux-tray.service >/dev/null 2>&1 || true
      echo "service : rtlinux-tray.service enabled"
    fi
  fi
fi

if command -v gtk-update-icon-cache >/dev/null; then
  gtk-update-icon-cache -f -t "${HOME}/.local/share/icons/hicolor" >/dev/null 2>&1 || true
fi
if command -v desktop-file-validate >/dev/null; then
  desktop-file-validate "${APP_DIR}/rtlinux.desktop" || true
fi

echo "== done =="
echo "binary : ${BIN_DIR}/rtlinux  (needs ${BIN_DIR} on PATH)"
echo "icon   : ${ICON_ROOT}/{16x16..512x512}/apps/rtlinux.png + scalable/apps/rtlinux.svg"
echo "menu   : ${APP_DIR}/rtlinux.desktop"
echo "tray   : ${BIN_DIR}/rtlinux-tray  (${APP_DIR}/rtlinux-tray.desktop)"
echo "config : ${HOME}/.config/rtlinux/config.toml"
echo "         (the tray writes it, and it starts with dry_run = true -"
echo "          it only watches until you turn that off in the tray)"
echo
echo "Log out and back in (or run 'update-desktop-database ~/.local/share/applications')"
echo "for the menu entry and icon to appear in Activities."
