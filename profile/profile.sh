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

DEPS=$(dirname $(readlink -f $0))/../deps
WAMR=$DEPS/wasm-micro-runtime

(
  cd $WAMR/build
  cmake -DCMAKE_BUILD_TYPE=Release ..
  make
)
(
  cd src-c
  . $DEPS/emsdk/emsdk_env.sh &>/dev/null
  emcc module-c.c -o module-c.wasm -O3 -s "TOTAL_MEMORY=1088KB" -s "TOTAL_STACK=1MB" --no-entry
  gcc container-wamr.c -o container-wamr -O3 -s -I$WAMR/core/iwasm/include -L$WAMR/build -lvmlib -lm -lpthread -lrt
)

cargo build --release --target "wasm32-unknown-unknown" --bin module-rust
cargo build --release --features="container" --bin container-wasmer
cargo build --release --features="container" --bin container-wasmi

cd working
ln -sf ../target/wasm32-unknown-unknown/release/module-rust.wasm .
ln -sf ../target/release/{container-wasmer,container-wasmi} .
ln -sf ../src-c/{container-wamr,module-c.wasm} .
echo

for ENGINE in wamr wasmer wasmi; do
  for MODULE in c rust; do
    ./container-$ENGINE module-$MODULE.wasm &
    PID=$!

    for _ in $(seq 100); do
      read TIME USER PROCESS MINFLT MAJFLT VSZ RSS MEM CMD <<<$(pidstat -r -p $PID | grep $PID)
      echo $((VSZ / 1024)) $((RSS / 1024))
      sleep 0.5
    done > $ENGINE-$MODULE.out

    kill $PID
    sleep 1
  done
done

gnuplot plot
xdg-open resident-memory.png
xdg-open virtual-memory.png
