#!/bin/bash
# The Linux x86-64 .deb and AppImage, built on this machine in an Ubuntu 22.04 container (glibc 2.35, so they run there and on anything newer; Docker under WSL on
# Windows) from the committed HEAD. Only the .dmg is built on GitHub (release-unix.yml): there is no Mac here.
#   scripts/build-linux-docker.sh <output folder> [cache folder]
# The cache keeps Rust, Node, the cargo target and the engine build between runs.
set -euo pipefail

if [ "${1:-}" = "--inside" ]; then
  [ "$(id -u)" -eq 0 ] || { echo "[ERROR] the container setup stage must run as root" >&2; exit 1; }
  BUILD_UID="${BUILD_UID:-}"
  BUILD_GID="${BUILD_GID:-}"
  [[ "$BUILD_UID" =~ ^[1-9][0-9]*$ && "$BUILD_GID" =~ ^[0-9]+$ ]] || {
    echo "[ERROR] pass the non-root host BUILD_UID and BUILD_GID into the container" >&2
    exit 1
  }
  export DEBIAN_FRONTEND=noninteractive
  echo "[..] packages"
  apt-get update -q >/dev/null
  apt-get install -y -q --no-install-recommends build-essential pkg-config cmake ninja-build nasm autoconf automake libtool \
    curl ca-certificates git unzip xz-utils libssl-dev libasound2-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
    librsvg2-dev patchelf file libfuse2 gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad \
    gstreamer1.0-libav libvulkan-dev passwd util-linux >/dev/null
  # Ubuntu 22.04 has no glslc and older Vulkan headers than the engine's ggml wants: LunarG's SDK has both
  vulkan=1.4.363.0
  if [ ! -x /cache/vulkansdk-$vulkan/x86_64/bin/glslc ]; then
    echo "[..] Vulkan SDK $vulkan"
    mkdir -p /cache/vulkansdk-$vulkan
    curl -sSfL "https://sdk.lunarg.com/sdk/download/$vulkan/linux/vulkansdk-linux-x86_64-$vulkan.tar.xz" | tar -xJ -C /cache/vulkansdk-$vulkan --strip-components=1
  fi
  export VULKAN_SDK=/cache/vulkansdk-$vulkan/x86_64 PATH=/cache/vulkansdk-$vulkan/x86_64/bin:$PATH
  # the engine's CUDA 13 backend: nvcc, the runtime and cuBLAS from NVIDIA's Ubuntu 22.04 repository, the packages kept in the cache
  cuda=13-3
  echo "[..] CUDA $cuda"
  curl -sSfL -o /tmp/cuda-keyring.deb https://developer.download.nvidia.com/compute/cuda/repos/ubuntu2204/x86_64/cuda-keyring_1.1-1_all.deb
  dpkg -i /tmp/cuda-keyring.deb >/dev/null
  mkdir -p /cache/apt
  apt-get update -q >/dev/null
  apt-get install -y -q --no-install-recommends -o Dir::Cache::Archives=/cache/apt cuda-nvcc-$cuda cuda-cudart-dev-$cuda libcublas-dev-$cuda >/dev/null
  # the CUDA 12 backend's toolkit beside it, from the same repository (the audio.cpp server's CUDA 12 build is 12.9 too)
  apt-get install -y -q --no-install-recommends -o Dir::Cache::Archives=/cache/apt cuda-nvcc-12-9 cuda-cudart-dev-12-9 libcublas-dev-12-9 >/dev/null
  build_cache="/cache/users/$BUILD_UID"
  build_home="$build_cache/home"
  if ! getent group "$BUILD_GID" >/dev/null; then
    groupadd --gid "$BUILD_GID" "yuebuild-$BUILD_GID"
  fi
  if ! getent passwd "$BUILD_UID" >/dev/null; then
    mkdir -p "$build_cache"
    useradd --uid "$BUILD_UID" --gid "$BUILD_GID" --home-dir "$build_home" --create-home --shell /bin/bash "yuebuild-$BUILD_UID"
  fi
  build_user="$(getent passwd "$BUILD_UID" | cut -d: -f1)"
  [ -n "$build_user" ] || { echo "[ERROR] could not resolve the build user for UID $BUILD_UID" >&2; exit 1; }
  mkdir -p "$build_cache" "$build_home"
  chown "$BUILD_UID:$BUILD_GID" "$build_cache" "$build_home"
  chown "$BUILD_UID:$BUILD_GID" /out
  find /out -maxdepth 1 -type f -exec chown "$BUILD_UID:$BUILD_GID" -- {} +
  runuser -u "$build_user" -- env -i \
    HOME="$build_home" USER="$build_user" LOGNAME="$build_user" \
    BUILD_UID="$BUILD_UID" BUILD_GID="$BUILD_GID" \
    VULKAN_SDK="/cache/vulkansdk-$vulkan/x86_64" CUDA12_ROOT=/usr/local/cuda-12.9 \
    PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    bash "$0" --build
  exit $?
fi

if [ "${1:-}" = "--build" ]; then
  BUILD_UID="${BUILD_UID:-}"
  BUILD_GID="${BUILD_GID:-}"
  [[ "$BUILD_UID" =~ ^[1-9][0-9]*$ && "$BUILD_GID" =~ ^[0-9]+$ ]] || {
    echo "[ERROR] the unprivileged build stage needs BUILD_UID and BUILD_GID" >&2
    exit 1
  }
  [ "$(id -u)" -eq "$BUILD_UID" ] || { echo "[ERROR] the build stage is not running as BUILD_UID=$BUILD_UID" >&2; exit 1; }
  BUILD_CACHE="/cache/users/$BUILD_UID"
  [ -d "$BUILD_CACHE" ] || { echo "[ERROR] the user build cache is missing: $BUILD_CACHE" >&2; exit 1; }
  export DEBIAN_FRONTEND=noninteractive APPIMAGE_EXTRACT_AND_RUN=1 RUSTUP_TOOLCHAIN=stable CARGO_TERM_COLOR=never
  export CARGO_HOME="$BUILD_CACHE/cargo" RUSTUP_HOME="$BUILD_CACHE/rustup" npm_config_cache="$BUILD_CACHE/npm"
  export CARGO_TARGET_DIR="$BUILD_CACHE/target" TMPDIR="$BUILD_CACHE/tmp"
  export PATH="$CARGO_HOME/bin:$BUILD_CACHE/node/bin:$VULKAN_SDK/bin:/usr/local/cuda-13.3/bin:$PATH"
  mkdir -p "$CARGO_HOME" "$RUSTUP_HOME" "$npm_config_cache" "$CARGO_TARGET_DIR" "$TMPDIR" "$BUILD_CACHE/node"
  if [ ! -x "$BUILD_CACHE/node/bin/node" ]; then
    echo "[..] node 22"
    v=$(curl -s https://nodejs.org/dist/latest-v22.x/ | grep -o 'node-v22[0-9.]*-linux-x64.tar.xz' | head -1)
    mkdir -p "$BUILD_CACHE/node" && curl -sL "https://nodejs.org/dist/latest-v22.x/$v" | tar -xJ -C "$BUILD_CACHE/node" --strip-components=1
  fi
  if [ ! -x "$CARGO_HOME/bin/cargo" ]; then
    echo "[..] rust stable"
    curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable >/dev/null
  fi
  node --version
  rustc --version
  export CUDA12_ROOT
  echo "[..] source $(git -c safe.directory=/src -C /src rev-parse --short HEAD)"
  rm -rf "$BUILD_CACHE/src" && mkdir -p "$BUILD_CACHE/src"
  git -c safe.directory=/src -C /src archive HEAD | tar -x -C "$BUILD_CACHE/src"
  cd "$BUILD_CACHE/src"
  ln -sfn "$CARGO_TARGET_DIR" desktop/src-tauri/target
  echo "[..] build (log: build.log)"
  bash scripts/build-release-linux.sh > /out/build.log 2>&1 || { echo "[ERROR] build failed"; tail -40 /out/build.log; exit 1; }
  engine=desktop/src-tauri/resources/yue2-cpp/yue-server
  if ldd "$engine" | grep -i 'not found'; then echo "[ERROR] the engine misses libraries"; exit 1; fi
  # libcuda.so.1 comes with the display driver, which a build container has not got
  backend=desktop/src-tauri/resources/yue2-cpp/cuda13/libggml-cuda.so
  [ -f "$backend" ] || { echo "[ERROR] the engine was built without its CUDA backend"; exit 1; }
  if ldd "$backend" | grep -i 'not found' | grep -v 'libcuda.so'; then echo "[ERROR] the CUDA backend misses libraries"; exit 1; fi
  backend12=desktop/src-tauri/resources/yue2-cpp/cuda12/libggml-cuda.so
  [ -f "$backend12" ] || { echo "[ERROR] the engine was built without its CUDA 12 backend"; exit 1; }
  # the engine answers --help with its usage and a non-zero code
  "$engine" --help > "$TMPDIR/engine-help.txt" 2>&1 || true
  grep -q 'Usage' "$TMPDIR/engine-help.txt" || { echo "[ERROR] the engine does not start"; head -5 "$TMPDIR/engine-help.txt"; exit 1; }
  midi=desktop/src-tauri/resources/music-midi/music-midi
  if ldd "$midi" | grep -i 'not found'; then echo "[ERROR] the MIDI transcriber misses libraries"; exit 1; fi
  caption=desktop/src-tauri/resources/music-midi/music-caption
  if ldd "$caption" | grep -i 'not found'; then echo "[ERROR] the captioner misses libraries"; exit 1; fi
  version="$(node -p "require('./desktop/src-tauri/tauri.conf.json').version")"
  product="$(node -p "require('./desktop/src-tauri/tauri.conf.json').productName")"
  # the cache may be shared with the other studios and keeps earlier versions: take this program's and this version's by full name
  deb="/cache/target/release/bundle/deb/${product}_${version}_amd64.deb"
  appimage="/cache/target/release/bundle/appimage/${product}_${version}_amd64.AppImage"
  [ -f "$deb" ] && [ -f "$appimage" ] || { echo "[ERROR] no ${product} ${version} package in the bundle folder"; ls -la /cache/target/release/bundle/deb /cache/target/release/bundle/appimage; exit 1; }
  # tauri-bundler names the package by heck::AsKebabCase(productName) and the menu entry and resource folder by productName
  package="$(node -p "process.argv[1].replace(/([a-z0-9])([A-Z])/g,'\$1 \$2').replace(/([A-Z]+)([A-Z][a-z])/g,'\$1 \$2').split(/[^A-Za-z0-9]+/).filter(Boolean).join('-').toLowerCase()" "$product")"
  found="$(dpkg-deb -f "$deb" Package)"
  [ "$found" = "$package" ] || { echo "[ERROR] $deb is the package $found, not $package"; exit 1; }
  listing="$(dpkg-deb -c "$deb")"
  for file in root.json url.txt; do
    grep -qF "usr/lib/${product}/component-channel/$file" <<< "$listing" || { echo "[ERROR] $deb lacks component-channel/$file"; exit 1; }
  done
  grep -qF " usr/share/applications/${product}.desktop" <<< "$listing" || { echo "[ERROR] $deb has no ${product}.desktop"; exit 1; }
  grep -qF " usr/lib/${product}/resources/yue2-cpp/yue-server" <<< "$listing" || { echo "[ERROR] $deb carries no YuE2 engine"; exit 1; }
  rm -rf "$TMPDIR/appimage-check" && mkdir -p "$TMPDIR/appimage-check"
  # the .desktop at the image's root is a link into usr/share/applications: take the file it points to
  (cd "$TMPDIR/appimage-check" && env -u APPIMAGE_EXTRACT_AND_RUN "$appimage" --appimage-extract 'usr/share/applications/*.desktop' >/dev/null)
  (cd "$TMPDIR/appimage-check" && env -u APPIMAGE_EXTRACT_AND_RUN "$appimage" --appimage-extract 'usr/lib/*/component-channel/*' >/dev/null)
  for file in root.json url.txt; do
    cmp "distribution/channels/stable/bootstrap/$file" "$TMPDIR/appimage-check/squashfs-root/usr/lib/$product/component-channel/$file"
  done
  grep -qx "Name=${product}" "$TMPDIR/appimage-check/squashfs-root/usr/share/applications"/*.desktop || { echo "[ERROR] $appimage is not ${product}"; exit 1; }
  echo "[OK] packages are ${product}: deb ${found}, AppImage $(basename "$TMPDIR"/appimage-check/squashfs-root/usr/share/applications/*.desktop)"
  cp "$deb" "/out/YuE2-Studio-${version}-linux-amd64.deb"
  cp "$appimage" "/out/YuE2-Studio-${version}-linux-x86_64.AppImage"
  ls -la /out/*.deb /out/*.AppImage
  echo "[OK] YuE2-Studio ${version} for Linux"
  exit 0
fi

if [ "$(id -u)" -eq 0 ]; then
  BUILD_UID="${BUILD_UID:-${SUDO_UID:-}}"
  BUILD_GID="${BUILD_GID:-${SUDO_GID:-}}"
else
  BUILD_UID="${BUILD_UID:-$(id -u)}"
  BUILD_GID="${BUILD_GID:-$(id -g)}"
fi
[[ "$BUILD_UID" =~ ^[1-9][0-9]*$ && "$BUILD_GID" =~ ^[0-9]+$ ]] || {
  echo "[ERROR] run as a regular user, or set BUILD_UID and BUILD_GID to that user's numeric IDs" >&2
  exit 1
}
out="${1:?output folder}"
build_home="$(getent passwd "$BUILD_UID" | cut -d: -f6)"
cache="${2:-${build_home:-$HOME}/yue2-linux2204-cache}"
repo="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$out" "$cache"
out="$(cd "$out" && pwd -P)"
cache="$(cd "$cache" && pwd -P)"
cp "$repo/scripts/build-linux-docker.sh" "$out/.build-linux-docker.sh"
docker run --rm --env BUILD_UID="$BUILD_UID" --env BUILD_GID="$BUILD_GID" \
  -v "$repo:/src:ro" -v "$out:/out" -v "$cache:/cache" \
  ubuntu:22.04 bash /out/.build-linux-docker.sh --inside
