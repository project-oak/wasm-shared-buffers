//
// Copyright 2021 The Project Oak Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
use common::*;
use std::{env, fs, io::prelude::*, process};
use wasmer_runtime::{Array, Ctx, func, Func, imports, Instance, instantiate, Memory, WasmPtr};

fn main() {
    let module_name = env::args().nth(1).expect("missing module name arg");
    let signal_index = env::args().nth(2).expect("missing signal index arg");
    println!("Container {} started (wasmer); module '{}', pid {}", signal_index, module_name, process::id());

    // Load and instantiate the wasm module.
    let mut bytes = Vec::new();
    fs::File::open(module_name).unwrap().read_to_end(&mut bytes).unwrap();

    let import_object = imports! {
        "env" => {
            "print_callback" => func!(|ctx: &mut Ctx, len: u32, msg: WasmPtr<u8, Array>| {
                println!("{}", msg.get_utf8_string(ctx.memory(0), len).unwrap());
            })
        }
    };
    let instance = instantiate(&bytes, &import_object)
        .expect("failed to instantiate wasm module");

    // Set up our shared memory buffers.
    let buffers = map_shared_buffers(&instance);

    let init: Func<i32> = instance.exports.get("init").unwrap();
    let tick: Func = instance.exports.get("tick").unwrap();
    let modify_grid: Func = instance.exports.get("modify_grid").unwrap();

    // Command loop. Comms does *not* take ownership of shared_rw.
    let mut comms = Comms::new(buffers.shared_rw, signal_index.parse().unwrap());
    loop {
        match comms.wait_for_signal() {
            Signal::Idle => panic!("unexpected idle signal"),
            Signal::Init => init.call(rand::random::<i32>()).expect("wasm call 'init' failed"),
            Signal::Tick => tick.call().expect("wasm call 'tick' failed"),
            Signal::ModifyGrid => modify_grid.call().expect("wasm call 'modify_grid' failed"),
            Signal::Exit => break,
        };
        comms.send_idle();
    }
    println!("Container {} stopping", signal_index);
}

fn map_shared_buffers(instance: &Instance) -> Buffers {
    // Call wasm.malloc to reserve enough space for the shared buffers plus alignment concerns.
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    let wasm_alloc_size = READ_ONLY_BUF_SIZE + READ_WRITE_BUF_SIZE + 3 * page_size as i32;

    let malloc: Func<i32, i32> = instance.exports.get("malloc").unwrap();
    let wasm_alloc_index = malloc.call(wasm_alloc_size).expect("malloc failed");

    // Get the location of wasm's linear memory buffer in our address space.
    let memory: Memory =  instance.exports.get("memory").unwrap();
    let wasm_memory_base = memory.view::<u8>().as_ptr() as i64;
    let wasm_alloc_ptr = wasm_memory_base + wasm_alloc_index as i64;

    // Align the shared buffers inside wasm's linear memory against our page boundaries.
    let aligned_ro_ptr = page_align(wasm_alloc_ptr, page_size);
    let aligned_rw_ptr = page_align(aligned_ro_ptr + READ_ONLY_BUF_SIZE as i64, page_size);

    // Map the buffers into the aligned locations.
    let buffers = Buffers::new(aligned_ro_ptr, aligned_rw_ptr);

    // Convert the aligned buffer locations into wasm linear memory indexes.
    // We want to skip the signal bytes when passing the r/w buffer into the wasm instance.
    let ro_index = (buffers.shared_ro as i64 - wasm_memory_base) as i32;
    let rw_index = (buffers.shared_rw as i64 - wasm_memory_base) as i32 + SIGNAL_BYTES;

    let set_shared: Func<(i32, i32, i32, i32)> = instance.exports.get("set_shared").unwrap();
    set_shared.call(ro_index, READ_ONLY_BUF_SIZE, rw_index, READ_WRITE_BUF_SIZE - SIGNAL_BYTES)
        .expect("set_shared failed");

    buffers
}
