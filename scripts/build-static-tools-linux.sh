#!/usr/bin/env bash
set -euo pipefail

NVIM_VERSION="${NVIM_VERSION:-0.11.5}"
FISH_VERSION="${FISH_VERSION:-4.3.3}"
TMUX_VERSION="${TMUX_VERSION:-3.6a}"
UTF8PROC_VERSION="${UTF8PROC_VERSION:-2.11.3}"
LIBEVENT_VERSION="${LIBEVENT_VERSION:-2.1.12-stable}"

HOST_ROOT="${BUILD_ROOT:-$PWD/.build/static-tools-linux}"
HOST_OUT="${OUTPUT_DIR:-$PWD/artifacts/static-tools-linux}"
CONTAINER_IMAGE="${LINUX_BUILD_IMAGE:-alpine:3.22}"

mkdir -p "$HOST_ROOT" "$HOST_OUT"

docker run --rm -i \
  -e NVIM_VERSION="$NVIM_VERSION" \
  -e FISH_VERSION="$FISH_VERSION" \
  -e TMUX_VERSION="$TMUX_VERSION" \
  -e UTF8PROC_VERSION="$UTF8PROC_VERSION" \
  -e LIBEVENT_VERSION="$LIBEVENT_VERSION" \
  -v "$HOST_ROOT:/work" \
  "$CONTAINER_IMAGE" \
  sh <<'SH'
set -eux

ROOT=/work
PREFIX="$ROOT/prefix"
SRC="$ROOT/src"
BUILD="$ROOT/build"
JOBS="$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)"

mkdir -p "$SRC" "$BUILD" "$PREFIX" "$ROOT/tmp"

apk add --no-cache \
  bash \
  bison \
  build-base \
  ca-certificates \
  cmake \
  coreutils \
  curl \
  file \
  git \
  linux-headers \
  musl-dev \
  ncurses-dev \
  ncurses-static \
  openssl-dev \
  patchelf \
  perl \
  pkgconf \
  python3 \
  rust \
  cargo \
  tar \
  xz

fetch() {
  url="$1"
  out="$2"
  test -f "$out" || curl -L --fail --retry 5 --retry-delay 3 -o "$out" "$url"
}

extract_once() {
  archive="$1"
  marker="$2"
  test -e "$marker" || tar -xf "$archive" -C "$BUILD"
}

fetch "https://github.com/neovim/neovim/archive/refs/tags/v$NVIM_VERSION.tar.gz" "$SRC/neovim-v$NVIM_VERSION.tar.gz"
fetch "https://github.com/fish-shell/fish-shell/releases/download/$FISH_VERSION/fish-$FISH_VERSION.tar.xz" "$SRC/fish-$FISH_VERSION.tar.xz"
fetch "https://github.com/tmux/tmux/releases/download/$TMUX_VERSION/tmux-$TMUX_VERSION.tar.gz" "$SRC/tmux-$TMUX_VERSION.tar.gz"
fetch "https://github.com/JuliaStrings/utf8proc/archive/refs/tags/v$UTF8PROC_VERSION.tar.gz" "$SRC/utf8proc-$UTF8PROC_VERSION.tar.gz"
fetch "https://github.com/libevent/libevent/releases/download/release-$LIBEVENT_VERSION/libevent-$LIBEVENT_VERSION.tar.gz" "$SRC/libevent-$LIBEVENT_VERSION.tar.gz"

extract_once "$SRC/neovim-v$NVIM_VERSION.tar.gz" "$BUILD/neovim-$NVIM_VERSION"
extract_once "$SRC/fish-$FISH_VERSION.tar.xz" "$BUILD/fish-$FISH_VERSION"
extract_once "$SRC/tmux-$TMUX_VERSION.tar.gz" "$BUILD/tmux-$TMUX_VERSION"
extract_once "$SRC/utf8proc-$UTF8PROC_VERSION.tar.gz" "$BUILD/utf8proc-$UTF8PROC_VERSION"
extract_once "$SRC/libevent-$LIBEVENT_VERSION.tar.gz" "$BUILD/libevent-$LIBEVENT_VERSION"

cd "$BUILD/utf8proc-$UTF8PROC_VERSION"
make libutf8proc.a prefix="$PREFIX"
make install prefix="$PREFIX"

cmake -S "$BUILD/libevent-$LIBEVENT_VERSION" -B "$BUILD/libevent-$LIBEVENT_VERSION-build" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$PREFIX" \
  -DBUILD_SHARED_LIBS=OFF \
  -DEVENT__DISABLE_OPENSSL=ON \
  -DEVENT__DISABLE_TESTS=ON \
  -DEVENT__DISABLE_SAMPLES=ON \
  -DEVENT__DISABLE_REGRESS=ON
cmake --build "$BUILD/libevent-$LIBEVENT_VERSION-build" -j"$JOBS"
cmake --install "$BUILD/libevent-$LIBEVENT_VERSION-build"

cd "$BUILD/tmux-$TMUX_VERSION"
./configure \
  --prefix="$PREFIX" \
  --enable-utf8proc \
  PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig" \
  CFLAGS="-O2 -I$PREFIX/include -I$PREFIX/include/ncursesw" \
  CPPFLAGS="-I$PREFIX/include -I/usr/include/ncursesw" \
  LDFLAGS="-L$PREFIX/lib -static" \
  LIBEVENT_CFLAGS="-I$PREFIX/include" \
  LIBEVENT_LIBS="$PREFIX/lib/libevent_core.a" \
  NCURSES_CFLAGS="-I/usr/include/ncursesw" \
  NCURSES_LIBS="/usr/lib/libncursesw.a" \
  UTF8PROC_CFLAGS="-I$PREFIX/include" \
  UTF8PROC_LIBS="$PREFIX/lib/libutf8proc.a"
make -j"$JOBS"
touch tmux.o
make tmux "LIBS=$PREFIX/lib/libutf8proc.a /usr/lib/libncursesw.a $PREFIX/lib/libevent_core.a -lm"
make install "LIBS=$PREFIX/lib/libutf8proc.a /usr/lib/libncursesw.a $PREFIX/lib/libevent_core.a -lm"

cd "$BUILD/fish-$FISH_VERSION"
export CARGO_HOME="$ROOT/cargo-home"
export CARGO_TARGET_DIR="$ROOT/cargo-target"
cmake -S . -B "$BUILD/fish-$FISH_VERSION-build" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$PREFIX" \
  -DFISH_USE_SYSTEM_PCRE2=OFF \
  -DCMAKE_EXE_LINKER_FLAGS="-static-libgcc -static-libstdc++" \
  -DWITH_DOCS=OFF
RUSTFLAGS="-C link-arg=-static-libgcc" \
  cmake --build "$BUILD/fish-$FISH_VERSION-build" --target fish fish_indent fish_key_reader -j"$JOBS"
RUSTFLAGS="-C link-arg=-static-libgcc" \
  cmake --build "$BUILD/fish-$FISH_VERSION-build" -j"$JOBS"
cmake --install "$BUILD/fish-$FISH_VERSION-build"
mkdir -p "$PREFIX/lib"
cp /usr/lib/libgcc_s.so.1 "$PREFIX/lib/"
patchelf --set-rpath '$ORIGIN/../lib' \
  "$PREFIX/bin/fish" \
  "$PREFIX/bin/fish_indent" \
  "$PREFIX/bin/fish_key_reader"

cd "$BUILD/neovim-$NVIM_VERSION"
make CMAKE_BUILD_TYPE=Release \
  CMAKE_INSTALL_PREFIX="$PREFIX" \
  CMAKE_EXTRA_FLAGS="-DENABLE_LIBINTL=OFF" \
  USE_BUNDLED=ON \
  DEPS_CMAKE_FLAGS="-DENABLE_WASMTIME=OFF" \
  -j"$JOBS"
cmake -S . -B build \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$PREFIX" \
  -DENABLE_LIBINTL=OFF \
  -DCMAKE_EXE_LINKER_FLAGS="-static -static-libgcc" \
  -DCMAKE_FIND_LIBRARY_SUFFIXES=.a
cmake --build build -j"$JOBS"
cmake --install "$BUILD/neovim-$NVIM_VERSION/build"

"$PREFIX/bin/nvim" --version
XDG_CACHE_HOME="$ROOT/tmp" "$PREFIX/bin/nvim" --clean --headless +q
"$PREFIX/bin/fish" --version
"$PREFIX/bin/fish" --no-config -c 'echo fish-smoke-ok'
"$PREFIX/bin/tmux" -V
TMUX_TMPDIR="$ROOT/tmp" "$PREFIX/bin/tmux" -L static-smoke -f /dev/null new-session -d "sleep 30"
TMUX_TMPDIR="$ROOT/tmp" "$PREFIX/bin/tmux" -L static-smoke has-session
TMUX_TMPDIR="$ROOT/tmp" "$PREFIX/bin/tmux" -L static-smoke kill-server

file "$PREFIX/bin/nvim" "$PREFIX/bin/fish" "$PREFIX/bin/tmux"
ldd "$PREFIX/bin/nvim" || true
ldd "$PREFIX/bin/fish" || true
ldd "$PREFIX/bin/tmux" || true
SH

rm -rf "$HOST_OUT"
mkdir -p "$HOST_OUT"
cp -a "$HOST_ROOT/prefix/." "$HOST_OUT/"

docker run --rm -i \
  -e FISH_VERSION="$FISH_VERSION" \
  -v "$HOST_ROOT:/work" \
  -v "$HOST_OUT:/out" \
  debian:trixie \
  bash <<'SH'
set -euxo pipefail

ROOT=/work
PREFIX=/work/glibc-fish-prefix
BUILD=/work/build
JOBS="$(nproc)"

apt-get update
DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
  build-essential \
  ca-certificates \
  cargo \
  cmake \
  file \
  git \
  patchelf \
  pkg-config \
  rustc

cd "$BUILD/fish-$FISH_VERSION"
export CARGO_HOME="$ROOT/cargo-home-glibc"
export CARGO_TARGET_DIR="$ROOT/cargo-target-glibc"
cmake -S . -B "$BUILD/fish-$FISH_VERSION-glibc-build" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$PREFIX" \
  -DFISH_USE_SYSTEM_PCRE2=OFF \
  -DCMAKE_EXE_LINKER_FLAGS="-static-libgcc -static-libstdc++" \
  -DWITH_DOCS=OFF
RUSTFLAGS="-C link-arg=-static-libgcc -C link-arg=-static-libstdc++" \
  cmake --build "$BUILD/fish-$FISH_VERSION-glibc-build" --target fish fish_indent fish_key_reader -j"$JOBS"
RUSTFLAGS="-C link-arg=-static-libgcc -C link-arg=-static-libstdc++" \
  cmake --build "$BUILD/fish-$FISH_VERSION-glibc-build" -j"$JOBS"
cmake --install "$BUILD/fish-$FISH_VERSION-glibc-build"

cp -a "$PREFIX/bin/fish" "$PREFIX/bin/fish_indent" "$PREFIX/bin/fish_key_reader" /out/bin/
rm -rf /out/share/fish
cp -a "$PREFIX/share/fish" /out/share/

/out/bin/fish --version
/out/bin/fish --no-config -c 'echo fish-glibc-smoke-ok'
file /out/bin/fish /out/bin/fish_indent /out/bin/fish_key_reader
ldd /out/bin/fish
SH

cat > "$HOST_OUT/BUILD-INFO.txt" <<EOF
tool versions:
  nvim: $NVIM_VERSION
  fish: $FISH_VERSION
  tmux: $TMUX_VERSION

build images:
  nvim/tmux: $CONTAINER_IMAGE
  fish: debian:trixie
build note:
  nvim and tmux are fully static musl Linux executables.
  fish is built for glibc Linux so it runs in OrbStack's default Arch Linux
  environment and common glibc distributions without a musl loader.
EOF
