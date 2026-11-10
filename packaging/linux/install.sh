#!/usr/bin/env bash
# Install openrtlinux for the current user: the binary, the icons, the menu entry.
#
#   bash packaging/linux/install.sh            # debug build (faster to rebuild)
#   bash packaging/linux/install.sh --release  # optimised build
#
# Nothing needs root. Icons go into the user's hicolor theme so the menu entry
# and any launcher can find "openrtlinux" by name.
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
  echo "== building openrtlinux (${PROFILE}) =="
  cd "${REPO}"
  cargo build --features linuxos --bin openrtlinux "${CARGO_FLAGS[@]}"
else
  echo "== skipping the build (--no-build) =="
fi

echo "== installing =="
install -Dm755 "${REPO}/target/${PROFILE}/openrtlinux" "${BIN_DIR}/openrtlinux"
# both states: the tray asks for `openrtlinux-off` whenever the daemon is stopped,
# and an icon that was never installed is a blank square in the panel
for size in 16 22 24 32 48 64 128 256 512; do
  for name in openrtlinux openrtlinux-off; do
    src="${HERE}/hicolor/${size}x${size}/apps/${name}.png"
    [[ -f "${src}" ]] || continue
    install -Dm644 "${src}" "${ICON_ROOT}/${size}x${size}/apps/${name}.png"
  done
done
[[ -f "${HERE}/hicolor/scalable/apps/openrtlinux.svg" ]] && \
  install -Dm644 "${HERE}/hicolor/scalable/apps/openrtlinux.svg" \
    "${ICON_ROOT}/scalable/apps/openrtlinux.svg"
[[ -f "${HERE}/openrtlinux.svg" ]] && \
  install -Dm644 "${HERE}/openrtlinux.svg" "${ICON_ROOT}/scalable/apps/openrtlinux.svg"
install -Dm644 "${HERE}/openrtlinux.desktop" "${APP_DIR}/openrtlinux.desktop"

# --- tray + settings window (Python/GTK3, talks to the daemon over its socket) ---
install -Dm755 "${REPO}/gui/openrtlinux_tray.py" "${BIN_DIR}/openrtlinux-tray"
install -Dm644 "${REPO}/gui/openrtlinux-tray.desktop" "${APP_DIR}/openrtlinux-tray.desktop"

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
install -Dm644 "${HERE}/openrtlinux.service" "${UNIT_DIR}/openrtlinux.service"
install -Dm644 "${HERE}/openrtlinux-tray.service" "${UNIT_DIR}/openrtlinux-tray.service"
if command -v systemctl >/dev/null && systemctl --user show-environment >/dev/null 2>&1; then
  systemctl --user daemon-reload || true
  if [[ "${ENABLE_SERVICE}" == "1" ]]; then
    systemctl --user enable --now openrtlinux.service >/dev/null 2>&1 || true
    echo "service : openrtlinux.service enabled (starts with the session)"
    if [[ "${ENABLE_TRAY}" == "1" ]]; then
      systemctl --user enable --now openrtlinux-tray.service >/dev/null 2>&1 || true
      echo "service : openrtlinux-tray.service enabled"
    fi
  fi
fi

if command -v gtk-update-icon-cache >/dev/null; then
  gtk-update-icon-cache -f -t "${HOME}/.local/share/icons/hicolor" >/dev/null 2>&1 || true
fi
if command -v desktop-file-validate >/dev/null; then
  desktop-file-validate "${APP_DIR}/openrtlinux.desktop" || true
  desktop-file-validate "${APP_DIR}/openrtlinux-tray.desktop" || true
fi

echo "== done =="
echo "binary : ${BIN_DIR}/openrtlinux  (needs ${BIN_DIR} on PATH)"
echo "icon   : ${ICON_ROOT}/{16x16..512x512}/apps/openrtlinux{,-off}.png + scalable/apps/openrtlinux.svg"
echo "menu   : ${APP_DIR}/openrtlinux-tray.desktop  (the only entry shown in Activities;"
echo "         ${APP_DIR}/openrtlinux.desktop is NoDisplay - the daemon runs as a service)"
echo "tray   : ${BIN_DIR}/openrtlinux-tray  (${APP_DIR}/openrtlinux-tray.desktop)"
echo "config : ${HOME}/.config/openrtlinux/config.toml"
echo "         (the tray writes it, and it starts with dry_run = true -"
echo "          it only watches until you turn that off in the tray)"
echo
echo "Log out and back in (or run 'update-desktop-database ~/.local/share/applications')"
echo "for the menu entry and icon to appear in Activities."
