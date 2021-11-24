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

BASE=$(dirname $(readlink -f $0))
WAMR=$BASE/deps/wasm-micro-runtime
EMSDK=$BASE/deps/emsdk

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

check_gtk4() {
  if ! pkg-config --validate gtk4 &>/dev/null; then
    echo "gtk4 dev libraries are required: please run 'sudo apt-get install libgtk-4-dev'"
    exit 1
  fi
}

build_wasm() {
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

build_gtk_wasm() {
  cd gtk-c
  for W in hunter runner; do
    build_wasm $W "-s TOTAL_MEMORY=16MB" module-common.c common.h
  done
  cd ..
}

build_container() {
  echo "Building container"
  gcc container.c -o container -I$WAMR/core/iwasm/include -L$WAMR/build -lvmlib -lm -lpthread -lrt
}

build_host() {
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
    check_gtk4
    build_gtk_wasm
    cd gtk-c
    build_container
    build_host $(pkg-config --cflags --libs gtk4)
    run
    ;;

  gr) # Rust-based GTK demo; uses wasm modules from gtk-c
    build_gtk_wasm
    cp -uv gtk-c/{hunter.wasm,runner.wasm} gtk-rust/
    cd gtk-rust
    cargo build
    cargo run
    ;;

  t) # Terminal-based tests (in C)
    setup_deps
    cd terminal
    build_wasm module "-s TOTAL_MEMORY=64KB -s TOTAL_STACK=1KB"
    build_container
    build_host
    run
    ;;

  clean)
    rm -vf {gtk-*,terminal}/{*.wasm,container,host} /dev/shm/{shared_ro,shared_rw}
    ;;

  *)  echo "Usage: gc | gr | t | clean"
      echo "  gc: GTK demo in C"
      echo "  gr: GTK demo in Rust"
      echo "  t: terminal-only tests"
esac