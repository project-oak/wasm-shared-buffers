#!/bin/bash
#
# Copyright 2021 The Project Oak Authors
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
#

set -ex

BASE=$(dirname $(readlink -f $0))
WAMR=$BASE/deps/wasm-micro-runtime
EMSDK=$BASE/deps/emsdk
RUST_WASM_TARGET="wasm32-unknown-unknown"
RUST_CONFIG="gtk-rust/Cargo.toml"
RUST_MODULES_OUT="gtk-rust/target/${RUST_WASM_TARGET}/debug"

setup_deps() {
  mkdir -p deps
  cd deps
  if [ ! -d wasm-micro-runtime ]; then
    echo "-- Setting up WAMR --"
    git clone https://github.com/bytecodealliance/wasm-micro-runtime.git
    mkdir wasm-micro-runtime/build
    cd wasm-micro-runtime/build
    cmake ..
    make
    cd ../..
    echo
  fi
  if [ ! -d emsdk ]; then
    echo "-- Setting up emsdk --"
    git clone https://github.com/emscripten-core/emsdk.git
    cd emsdk
    ./emsdk install latest
    ./emsdk activate latest
    cd ..
    echo
  fi
  cd ..
}

get_rust_tooling() {
  if ! rustup -V &>/dev/null; then
    echo "Installing Rustup"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    source ~/.cargo/env
  fi

  echo "Ensuring we have the latest rust tool chains"
  rustup toolchain install stable
  echo "Set stable as default"
  rustup default stable

  echo "Installing Wasm target for Rust"
  rustup target add "$RUST_WASM_TARGET"

  echo "Done"
}

check_gtk4() {
  if ! pkg-config --validate gtk4 &>/dev/null; then
    echo "gtk4 dev libraries are required: please run 'sudo apt-get install libgtk-4-dev'"
    exit 1
  fi
}

build_wasm_c() {
  local F NAME=$1 FLAGS="$2"
  for F in $NAME.c "${@:3}"; do
    if [ $F -nt $NAME.wasm ]; then
      echo "Building $NAME.wasm"
      . $EMSDK/emsdk_env.sh &>/dev/null
      emcc --no-entry -s EXPORTED_FUNCTIONS="['_malloc']" $FLAGS -Os $NAME.c -o $NAME.wasm
      break
    fi
  done
}

build_gtk_wasm_rust() {
  cargo build --target "$RUST_WASM_TARGET" --manifest-path "$RUST_CONFIG" --features modules
}

build_gtk_wasm_c() {
  check_gtk4
  cd gtk-c
  for W in hunter runner; do
    build_wasm_c $W "-s TOTAL_MEMORY=16MB" module-common.c common.h
  done
  cd ..
}

build_host() {
  echo "Building container"
  gcc container.c -o container -I$WAMR/core/iwasm/include -L$WAMR/build -lvmlib -lm -lpthread -lrt
  echo "Building host"
  gcc $1 host.c -o host "$@" -lrt
}

run() {
  echo -e "\n-- Running --"
  rm -f /dev/shm/{shared_ro,shared_rw}
  ./host "$@"
}

case "$1" in
  gc) # C-based GTK demo
    setup_deps
    build_gtk_wasm_c
    cd gtk-c
    build_host $(pkg-config --cflags --libs gtk4)
    run hunter.wasm runner.wasm
    ;;

  grc) # Rust-based GTK demo; uses wasm modules from gtk-c
    setup_deps
    build_gtk_wasm_c
    cargo build --manifest-path "$RUST_CONFIG" --features host
    cargo run --manifest-path "$RUST_CONFIG" --features host gtk-c/hunter.wasm gtk-c/runner.wasm
    ;;

  gcr) # C-based GTK demo with Rust wasm modules
    setup_deps
    build_gtk_wasm_rust
    cd gtk-c
    build_host $(pkg-config --cflags --libs gtk4)
    run "../${RUST_MODULES_OUT}/hunter.wasm" "../${RUST_MODULES_OUT}/runner.wasm"
    # run "../${RUST_MODULES_OUT}/runner.wasm" "../${RUST_MODULES_OUT}/hunter.wasm"
    ;;

  gr) # Rust-based GTK demo; uses wasm modules from gtk-rust
    setup_deps
    build_gtk_wasm_rust
    cargo build --manifest-path "$RUST_CONFIG" --features host
    ./gtk-rust/target/debug/host "${RUST_MODULES_OUT}/hunter.wasm" "${RUST_MODULES_OUT}/runner.wasm"
    ;;

  t) # Terminal-based tests (in C)
    setup_deps
    cd terminal
    build_wasm_c module "-s TOTAL_MEMORY=64KB -s TOTAL_STACK=1KB"
    build_host
    run
    ;;

  i) # Install any deps needed
    setup_deps
    get_rust_tooling
    ;;

  clean)
    rm -vf {gtk-*,terminal}/{*.wasm,container,host} /dev/shm/{shared_ro,shared_rw}
    ( cd gtk-rust && cargo clean )
    ;;

  *)  echo "Usage: gc | gr | grc | gcr | t | i | clean"
      echo "  gc: GTK demo in C"
      echo "  gr: GTK demo in Rust"
      echo "  grc: GTK demo with Rust host and C wasm modules"
      echo "  gcr: GTK demo with C host and Rust wasm modules"
      echo "  t: terminal-only tests"
      echo "  i: install dependencies"
      echo "  clean: cleans up build artifacts"
esac
