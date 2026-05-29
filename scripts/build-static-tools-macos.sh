#!/usr/bin/env bash
set -euo pipefail

NVIM_VERSION="${NVIM_VERSION:-0.11.5}"
FISH_VERSION="${FISH_VERSION:-4.3.3}"
TMUX_VERSION="${TMUX_VERSION:-3.6a}"
UTF8PROC_VERSION="${UTF8PROC_VERSION:-2.11.3}"

ROOT="${BUILD_ROOT:-$PWD/.build/static-tools-macos}"
PREFIX="${OUTPUT_DIR:-$PWD/artifacts/static-tools-macos}"
SRC="$ROOT/src"
BUILD="$ROOT/build"
JOBS="$(sysctl -n hw.ncpu 2>/dev/null || echo 4)"
BREW_PREFIX="$(brew --prefix)"

mkdir -p "$SRC" "$BUILD" "$PREFIX" "$ROOT/tmp" "$ROOT/tmux-tmp"

brew update
brew install cmake pkg-config autoconf automake libtool rust libevent ncurses gettext

fetch() {
  local url="$1"
  local out="$2"
  test -f "$out" || curl -L --fail --retry 5 --retry-delay 3 -o "$out" "$url"
}

extract_once() {
  local archive="$1"
  local marker="$2"
  test -e "$marker" || tar -xf "$archive" -C "$BUILD"
}

fetch "https://github.com/neovim/neovim/archive/refs/tags/v$NVIM_VERSION.tar.gz" "$SRC/neovim-v$NVIM_VERSION.tar.gz"
fetch "https://github.com/fish-shell/fish-shell/releases/download/$FISH_VERSION/fish-$FISH_VERSION.tar.xz" "$SRC/fish-$FISH_VERSION.tar.xz"
fetch "https://github.com/tmux/tmux/releases/download/$TMUX_VERSION/tmux-$TMUX_VERSION.tar.gz" "$SRC/tmux-$TMUX_VERSION.tar.gz"
fetch "https://github.com/JuliaStrings/utf8proc/archive/refs/tags/v$UTF8PROC_VERSION.tar.gz" "$SRC/utf8proc-$UTF8PROC_VERSION.tar.gz"

extract_once "$SRC/neovim-v$NVIM_VERSION.tar.gz" "$BUILD/neovim-$NVIM_VERSION"
extract_once "$SRC/fish-$FISH_VERSION.tar.xz" "$BUILD/fish-$FISH_VERSION"
extract_once "$SRC/tmux-$TMUX_VERSION.tar.gz" "$BUILD/tmux-$TMUX_VERSION"
extract_once "$SRC/utf8proc-$UTF8PROC_VERSION.tar.gz" "$BUILD/utf8proc-$UTF8PROC_VERSION"

cd "$BUILD/utf8proc-$UTF8PROC_VERSION"
make libutf8proc.a prefix="$PREFIX"
make install prefix="$PREFIX"

cd "$BUILD/tmux-$TMUX_VERSION"
./configure \
  --prefix="$PREFIX" \
  --enable-utf8proc \
  PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig:$BREW_PREFIX/opt/libevent/lib/pkgconfig:$BREW_PREFIX/opt/ncurses/lib/pkgconfig" \
  CFLAGS="-O2 -I$PREFIX/include" \
  CPPFLAGS="-I$PREFIX/include" \
  LDFLAGS="-L$PREFIX/lib" \
  LIBEVENT_CFLAGS="-I$BREW_PREFIX/opt/libevent/include" \
  LIBEVENT_LIBS="$BREW_PREFIX/opt/libevent/lib/libevent_core.a" \
  NCURSES_CFLAGS="-I$BREW_PREFIX/opt/ncurses/include" \
  NCURSES_LIBS="$BREW_PREFIX/opt/ncurses/lib/libncursesw.a" \
  LIBUTF8PROC_CFLAGS="-I$PREFIX/include" \
  LIBUTF8PROC_LIBS="$PREFIX/lib/libutf8proc.a" \
  UTF8PROC_CFLAGS="-I$PREFIX/include" \
  UTF8PROC_LIBS="$PREFIX/lib/libutf8proc.a"
make -j"$JOBS"
touch tmux.o
make tmux "LIBS=$PREFIX/lib/libutf8proc.a $BREW_PREFIX/opt/ncurses/lib/libncursesw.a $BREW_PREFIX/opt/libevent/lib/libevent_core.a -lm -lresolv"
make install "LIBS=$PREFIX/lib/libutf8proc.a $BREW_PREFIX/opt/ncurses/lib/libncursesw.a $BREW_PREFIX/opt/libevent/lib/libevent_core.a -lm -lresolv"

cd "$BUILD/fish-$FISH_VERSION"
export CARGO_HOME="$ROOT/cargo-home"
export CARGO_TARGET_DIR="$ROOT/cargo-target"
cmake -S . -B "$BUILD/fish-$FISH_VERSION-build" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$PREFIX" \
  -DFISH_USE_SYSTEM_PCRE2=OFF \
  -DWITH_DOCS=OFF
cmake --build "$BUILD/fish-$FISH_VERSION-build" --target fish fish_indent fish_key_reader -j"$JOBS"
cmake --build "$BUILD/fish-$FISH_VERSION-build" -j"$JOBS"
cmake --install "$BUILD/fish-$FISH_VERSION-build"

cd "$BUILD/neovim-$NVIM_VERSION"
make CMAKE_BUILD_TYPE=Release \
  CMAKE_INSTALL_PREFIX="$PREFIX" \
  USE_BUNDLED=ON \
  DEPS_CMAKE_FLAGS="-DENABLE_WASMTIME=OFF" \
  -j"$JOBS"

LINK_TXT="$BUILD/neovim-$NVIM_VERSION/build/src/nvim/CMakeFiles/nvim_bin.dir/link.txt"
if test -f "$BREW_PREFIX/opt/gettext/lib/libintl.a" && test -f "$LINK_TXT"; then
  perl -0pi -e "s#\Q$BREW_PREFIX\E/opt/gettext/lib/libintl\.dylib#$BREW_PREFIX/opt/gettext/lib/libintl.a#g" "$LINK_TXT"
  touch "$BUILD/neovim-$NVIM_VERSION/build/src/nvim/CMakeFiles/nvim_bin.dir/version.c.o"
  cmake --build "$BUILD/neovim-$NVIM_VERSION/build" --target nvim_bin
fi
cmake --install "$BUILD/neovim-$NVIM_VERSION/build"

"$PREFIX/bin/nvim" --version
XDG_CACHE_HOME="$ROOT/tmp" "$PREFIX/bin/nvim" --clean --headless +q
"$PREFIX/bin/fish" --version
"$PREFIX/bin/fish" --no-config -c 'echo fish-smoke-ok'
"$PREFIX/bin/tmux" -V
TMUX_TMPDIR="$ROOT/tmux-tmp" "$PREFIX/bin/tmux" -L static-smoke -f /dev/null new-session -d "sleep 30"
TMUX_TMPDIR="$ROOT/tmux-tmp" "$PREFIX/bin/tmux" -L static-smoke has-session
TMUX_TMPDIR="$ROOT/tmux-tmp" "$PREFIX/bin/tmux" -L static-smoke kill-server

file "$PREFIX/bin/nvim" "$PREFIX/bin/fish" "$PREFIX/bin/tmux"
otool -L "$PREFIX/bin/nvim"
otool -L "$PREFIX/bin/fish"
otool -L "$PREFIX/bin/tmux"

cat > "$PREFIX/BUILD-INFO.txt" <<EOF
tool versions:
  nvim: $NVIM_VERSION
  fish: $FISH_VERSION
  tmux: $TMUX_VERSION

build note:
  macOS does not support fully static userland binaries.
  Third-party dependencies are linked statically where practical; Darwin
  system libraries and frameworks remain dynamic.
EOF
