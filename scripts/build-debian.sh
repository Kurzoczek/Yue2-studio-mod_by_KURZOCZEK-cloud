#!/bin/bash
# YuE2 Studio 5.0 — budowa i instalacja paczki .deb na Debianie 13 (trixie).
# Silnik: Vulkan (Intel/AMD) + procesor, bez CUDA. Budowa idzie w tym samym folderze, w którym są źródła,
# jako zwykły użytkownik; narzędzia i pliki pośrednie (~25 GB) trafiają do podfolderu .debian-build.
#
#   sudo bash scripts/build-debian.sh            (z folderu źródeł)
#
# Zmienne (opcjonalnie): BUILD_USER=<konto>  BUILD_JOBS=2  NO_INSTALL=1
# Silnik YuE2 sam dobiera liczbę równoległych kompilacji do wolnego RAM-u (YUE_BUILD_JOBS ją ogranicza).
set -euo pipefail

SRC="$(cd "$(dirname "$0")/.." && pwd)"
BUILD_USER="${BUILD_USER:-${SUDO_USER:-}}"
BUILD_JOBS="${BUILD_JOBS:-2}"          # 8 GB RAM: więcej równoległych kompilacji = ryzyko braku pamięci
WORK="$SRC/.debian-build"

log() { echo; echo "[$(date +%H:%M:%S)] $*"; }

# ======================= ETAP UŻYTKOWNIKA (bez roota) =======================
if [ "${1:-}" = "--as-user" ]; then
  cd "$SRC"
  export CARGO_HOME="$WORK/cargo" RUSTUP_HOME="$WORK/rustup" npm_config_cache="$WORK/npm-cache"
  export TMPDIR="$WORK/tmp" YUE_ENGINE_BUILD_ROOT="$WORK/engines" CARGO_TARGET_DIR="$WORK/target"
  export CARGO_BUILD_JOBS="$BUILD_JOBS" BUILD_JOBS BUNDLES=deb
  export PATH="$WORK/node/bin:$CARGO_HOME/bin:$PATH"
  mkdir -p "$CARGO_HOME" "$RUSTUP_HOME" "$npm_config_cache" "$TMPDIR" "$YUE_ENGINE_BUILD_ROOT" "$CARGO_TARGET_DIR"

  log "[u1/4] Rust stable (rustup z Debiana, bez curl | sh)"
  rustup set profile minimal >/dev/null
  rustup toolchain install stable --profile minimal -c rustfmt -c clippy
  rustup default stable >/dev/null
  rustc --version

  log "[u2/4] Node.js 22 (oficjalne archiwum, suma SHA-256 sprawdzana)"
  if [ ! -x "$WORK/node/bin/node" ]; then
    base=https://nodejs.org/dist/latest-v22.x
    curl -fsSL "$base/SHASUMS256.txt" -o "$TMPDIR/SHASUMS256.txt"
    name="$(grep -o 'node-v22[0-9.]*-linux-x64.tar.xz' "$TMPDIR/SHASUMS256.txt" | head -1)"
    [ -n "$name" ] || { echo "[BŁĄD] nie znalazłem archiwum Node 22 w SHASUMS256.txt"; exit 1; }
    curl -fSL --progress-bar "$base/$name" -o "$TMPDIR/$name"
    (cd "$TMPDIR" && grep "  $name\$" SHASUMS256.txt | sha256sum -c -) || { echo "[BŁĄD] suma SHA-256 Node się nie zgadza"; exit 1; }
    mkdir -p "$WORK/node" && tar -xJf "$TMPDIR/$name" -C "$WORK/node" --strip-components=1
    rm -f "$TMPDIR/$name"
  fi
  node --version

  log "[u3/4] Budowa silnika (Vulkan + CPU) i studia — na N150 to potrwa 1–3 h. Log: $WORK/build.log"
  bash scripts/build-release-linux.sh 2>&1 | tee "$WORK/build.log"

  log "[u4/4] Sprawdzenie wyniku"
  deb="$(ls -t "$CARGO_TARGET_DIR"/release/bundle/deb/*.deb 2>/dev/null | head -1)"
  [ -n "$deb" ] || { echo "[BŁĄD] brak paczki .deb — zobacz $WORK/build.log"; exit 1; }
  engine=desktop/src-tauri/resources/yue2-cpp/yue-server
  if ldd "$engine" | grep -i 'not found'; then echo "[BŁĄD] silnikowi brakuje bibliotek"; exit 1; fi
  # yue-server wypisuje pomoc i kończy się kodem 1, więc przy pipefail nie wolno go puszczać w potok z grep
  help="$("$engine" --help 2>&1 || true)"
  grep -q Usage <<<"$help" || { echo "[BŁĄD] silnik się nie uruchamia:"; echo "$help" | head -20; exit 1; }
  cp "$deb" "$SRC/"
  echo "[OK] $(basename "$deb") → $SRC/"
  exit 0
fi

# ============================ ETAP ROOTA ============================
[ "$(id -u)" -eq 0 ] || { echo "Uruchom jako root: sudo bash $0"; exit 1; }
[ -n "$BUILD_USER" ] || { echo "Ustaw BUILD_USER=... albo uruchom skrypt przez sudo z konta użytkownika."; exit 1; }
id "$BUILD_USER" >/dev/null 2>&1 || { echo "Nie ma użytkownika $BUILD_USER — ustaw BUILD_USER=..."; exit 1; }

log "[1/4] Pakiety Debiana"
apt-get update -q
DEBIAN_FRONTEND=noninteractive apt-get install -y -q --no-install-recommends \
  build-essential pkg-config cmake ninja-build nasm autoconf automake libtool ccache \
  curl wget ca-certificates git unzip xz-utils file patchelf util-linux \
  libssl-dev libasound2-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev \
  libvulkan-dev glslc mesa-vulkan-drivers \
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav \
  rustup

log "[2/4] Folder budowy: $SRC"
mkdir -p "$WORK"
chown -R "$BUILD_USER:" "$SRC" 2>/dev/null || true
free_gb=$(df -BG --output=avail "$SRC" | tail -1 | tr -dc 0-9)
if [ "$free_gb" -lt 30 ]; then
  echo "[BŁĄD] na dysku z $SRC jest wolne ${free_gb} GB, a budowa zajmuje ~25 GB."
  exit 1
fi
# użytkownik budowy musi tu pisać, uruchamiać programy i tworzyć dowiązania symboliczne (Node, npm, biblioteki
# silnika): FAT/exFAT albo dysk zamontowany z noexec się nie nadadzą
probe="$WORK/.probe-$$"
chown "$BUILD_USER:" "$WORK" 2>/dev/null || true
# wszystko w teście robi sam użytkownik budowy (folder testu założony przez roota byłby dla niego zamknięty)
if ! runuser -u "$BUILD_USER" -- sh -c "mkdir -p '$probe.d' && printf '#!/bin/sh\nexit 0\n' > '$probe' && chmod +x '$probe' && '$probe' && ln -s '$probe' '$probe.d/link' && [ -x '$probe.d/link' ]"; then
  rm -rf "$probe" "$probe.d"
  echo "[BŁĄD] W $SRC użytkownik $BUILD_USER nie może pisać, uruchamiać programów albo tworzyć dowiązań"
  echo "       symbolicznych (system plików: $(findmnt -no FSTYPE -T "$SRC"), opcje: $(findmnt -no OPTIONS -T "$SRC"))."
  echo "       Potrzebny jest dysk z ext4 albo btrfs, zamontowany bez noexec."
  exit 1
fi
rm -rf "$probe" "$probe.d"

log "[3/4] Budowa jako $BUILD_USER (nie root)"
HOME_DIR="$(getent passwd "$BUILD_USER" | cut -d: -f6)"
runuser -u "$BUILD_USER" -- env -i HOME="$HOME_DIR" USER="$BUILD_USER" LANG="${LANG:-C.UTF-8}" \
  PATH=/usr/local/bin:/usr/bin:/bin BUILD_JOBS="$BUILD_JOBS" \
  bash "$SRC/scripts/build-debian.sh" --as-user

deb="$(ls -t "$SRC"/YuE2*_amd64.deb 2>/dev/null | head -1)"
[ -n "$deb" ] || { echo "[BŁĄD] nie ma paczki w $SRC"; exit 1; }
if [ "${NO_INSTALL:-0}" = "1" ]; then
  echo "Paczka gotowa: $deb (NO_INSTALL=1 — nie instaluję)"; exit 0
fi

log "[4/4] Instalacja paczki i bezpiecznego programu uruchamiającego"
apt-get install -y "$deb"
install -m 0755 "$SRC/scripts/yue2-studio-debian" /usr/local/bin/yue2-studio
# wpis w menu: ten sam identyfikator co z paczki, ale uruchamia program pośredniczący
src_desktop="$(ls /usr/share/applications/*YuE2*.desktop 2>/dev/null | head -1)"
if [ -n "$src_desktop" ]; then
  mkdir -p /usr/local/share/applications
  sed 's#^Exec=[^ ]*#Exec=/usr/local/bin/yue2-studio#' "$src_desktop" > "/usr/local/share/applications/$(basename "$src_desktop")"
fi
echo
echo "[OK] YuE2 Studio zainstalowane. Uruchamiaj jako $BUILD_USER: z menu albo poleceniem  yue2-studio"
echo "     Pliki budowy: $WORK (można usunąć; zostawione przyspieszą następną budowę)."
