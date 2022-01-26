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

set -e

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

build_wasm_c() {
  local F NAME=$1 FLAGS="$2"
  . $EMSDK/emsdk_env.sh &>/dev/null
  for F in $NAME.c "${@:3}"; do
    if [ $F -nt $NAME.wasm ]; then
      echo "Building $NAME.wasm"
      emcc --no-entry -s EXPORTED_FUNCTIONS="['_malloc']" $FLAGS -Os $NAME.c -o $NAME.wasm
      break
    fi
  done
}

build_gtk_wasm_c() {
  if ! pkg-config --validate gtk4 &>/dev/null; then
    echo "gtk4 dev libraries are required: please run 'sudo apt-get install libgtk-4-dev'"
    exit 1
  fi
  cd c/gtk
  for W in hunter runner; do
    build_wasm_c $W "-s TOTAL_MEMORY=16MB" module-common.c common.h
  done
  cd ../..
}

build_gtk_wasm_rust() {
  cargo build $MODE_FLAG --target "$RUST_WASM_TARGET" --manifest-path "$RUST_CONFIG" --features modules
}

build_wasm_container() {
  echo "Building container"
  gcc container.c -o container -I$WAMR/core/iwasm/include -L$WAMR/build -lvmlib -lm -lpthread -lrt
}

build_wasm_host() {
  echo "Building host"
  gcc host.c -o host $(pkg-config --cflags --libs gtk4) -lrt
}

run() {
  echo -e "\n-- Running --"
  rm -f /dev/shm/{shared_ro,shared_rw}
  ./host "$@"
}

# Handle the command line arguments
MODE="debug"
MODE_FLAG=""
for i in "$@"; do
  case $i in
    -r)
      MODE="release"
      MODE_FLAG="--release"
      shift
      ;;
    *)
      CMD="$i"
      ;;
  esac
done

BASE=$(dirname $(readlink -f $0))
WAMR=$BASE/deps/wasm-micro-runtime
EMSDK=$BASE/deps/emsdk
RUST_WASM_TARGET="wasm32-unknown-unknown"
RUST_CONFIG="rust/gtk/Cargo.toml"
RUST_MODULES_OUT="rust/gtk/target/${RUST_WASM_TARGET}/${MODE}"

case "$CMD" in
  gc) # C GTK demo
    setup_deps
    build_gtk_wasm_c
    cd c/gtk
    build_wasm_container
    build_wasm_host
    ./host hunter.wasm runner.wasm
    ;;

  gr) # Rust GTK demo
    setup_deps
    build_gtk_wasm_rust
    cargo build $MODE_FLAG --manifest-path "$RUST_CONFIG" --features host
    ./rust/gtk/target/${MODE}/host "${RUST_MODULES_OUT}/hunter.wasm" "${RUST_MODULES_OUT}/runner.wasm"
    ;;

  grc) # Rust GTK host/container with C wasm modules
    setup_deps
    build_gtk_wasm_c
    cargo build $MODE_FLAG --manifest-path "$RUST_CONFIG" --features host
    cargo run $MODE_FLAG --manifest-path "$RUST_CONFIG" --features host c/gtk/hunter.wasm c/gtk/runner.wasm
    ;;

  gcr) # C GTK host/container with Rust wasm modules
    setup_deps
    build_gtk_wasm_rust
    cd c/gtk
    build_wasm_host
    ./host "../../${RUST_MODULES_OUT}/hunter.wasm" "../../${RUST_MODULES_OUT}/runner.wasm"
    ;;

  h) # Heap guard demo
    cd c/heap-guard
    build_wasm_c module "-s TOTAL_MEMORY=64KB -s TOTAL_STACK=16KB"
    build_wasm_container
    echo -e "\n-- Without heap guard --"
    ./container
    echo -e "\n-- With heap guard --"
    ./container +
    ;;

  l) # Lookup store comparison
    cd rust/lookup
    cargo build --bin reader --target wasm32-unknown-unknown
    cargo run --bin lookup --features lookup -- target/wasm32-unknown-unknown/debug/reader.wasm
    ;;

  t) # Terminal tests
    setup_deps
    cd terminal
    build_wasm_c module "-s TOTAL_MEMORY=64KB -s TOTAL_STACK=1KB"
    build_wasm_container
    gcc host.c -o host -lrt
    ./host
    ;;

  i) # Install any deps needed
    setup_deps
    get_rust_tooling
    ;;

  clean)
    rm -vf {c/{gtk,heap-guard},terminal}/{*.wasm,container,host} /dev/shm/{shared_ro,shared_rw}
    ( cd rust/gtk && cargo clean -v )
    ( cd rust/lookup && cargo clean -v )
    ;;

  *)  echo "Usage: ./run.sh [-r] (gc | gr | grc | gcr | h | t | i | clean)"
      echo "  gc: GTK demo in C"
      echo "  gr: GTK demo in Rust"
      echo "  grc: GTK demo with Rust host and C wasm modules"
      echo "  gcr: GTK demo with C host and Rust wasm modules"
      echo "  h: Heap guard demo"
      echo "  t: terminal-only tests"
      echo "  i: install dependencies"
      echo "  clean: cleans up build artifacts"
      echo "  -r: use release mode for rust"
esac
