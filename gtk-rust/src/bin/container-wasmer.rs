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
use common::host_common::*;
use std::{env, fs, io::prelude::*, process};
use wasmer_runtime::{func, imports, instantiate, Array, Ctx, Func, Instance, Memory, WasmPtr};

fn main() {
    let module_name = env::args().nth(1).expect("missing module name arg");
    let index = env::args().nth(2).expect("missing signal index arg");
    println!("Container {} started (wasmer); module '{}', pid {}", index, module_name, process::id());

    // Load and instantiate the wasm module.
    let instance = {
        let mut bytes = Vec::new();
        fs::File::open(module_name).unwrap().read_to_end(&mut bytes).unwrap();
        let import_object = imports! {
            "env" => {
                "print_callback" => func!(|ctx: &mut Ctx, len: u32, msg: WasmPtr<u8, Array>| {
                    print!("{}", msg.get_utf8_string(ctx.memory(0), len).unwrap());
                })
            }
        };
        instantiate(&bytes, &import_object).expect("failed to instantiate wasm module")
    };

    // Set up our shared memory buffers and create the wasm module's context object.
    let w = map_shared_buffers(&instance, index.parse().unwrap());

    let init: Func<(i32, i32)> = instance.exports.get("init").unwrap();
    let tick: Func<i32> = instance.exports.get("tick").unwrap();
    let modify_grid: Func<i32> = instance.exports.get("modify_grid").unwrap();

    // Command loop.
    loop {
        let signal = w.buffers.wait_for_signal();

        if w.wasm_memory_base != get_wasm_memory_base(&instance) {
            println!("Container {}: linear buffer memory address changed!", index);
            break;
        }

        match signal {
            Signal::Idle => panic!("unexpected idle signal"),
            Signal::Init => init.call(w.wasm_context, rand::random::<i32>()).expect("wasm call 'init' failed"),
            Signal::Tick => tick.call(w.wasm_context).expect("wasm call 'tick' failed"),
            Signal::LargeAlloc => (), // resize handling not implemented for wasmer
            Signal::ModifyGrid => modify_grid.call(w.wasm_context).expect("wasm call 'modify_grid' failed"),
            Signal::Exit => break,
        };
        w.buffers.send_idle();
    }
    println!("Container {} stopping", index);
}

struct Wrapper {
    buffers: Buffers,
    wasm_context: i32,
    wasm_memory_base: i64,
}

fn map_shared_buffers(instance: &Instance, index: usize) -> Wrapper {
    // Call wasm.malloc to reserve enough space for the shared buffers plus alignment concerns.
    let wasm_malloc: Func<i32, i32> = instance.exports.get("malloc_").unwrap();
    let wasm_alloc_index = wasm_malloc.call(WASM_ALLOC_SIZE).expect("malloc_ failed");

    // Get the location of wasm's linear memory buffer in our address space.
    let wasm_memory_base = get_wasm_memory_base(instance);
    let wasm_alloc_ptr = wasm_memory_base + wasm_alloc_index as i64;

    // Align the shared buffers inside wasm's linear memory against our page boundaries.
    let aligned_ro_ptr = page_align(wasm_alloc_ptr);
    let aligned_rw_ptr = page_align(aligned_ro_ptr + READ_ONLY_BUF_SIZE as i64);

    // Map the buffers into the aligned locations.
    let shared_ro = map_buffer(aligned_ro_ptr, READ_ONLY_BUF_NAME, READ_ONLY_BUF_SIZE, true);
    let shared_rw = map_buffer(aligned_rw_ptr, READ_WRITE_BUF_NAME, READ_WRITE_BUF_SIZE, false);
    assert_eq!(shared_ro as i64, aligned_ro_ptr);
    assert_eq!(shared_rw as i64, aligned_rw_ptr);
    let buffers = Buffers::new(shared_ro, shared_rw, index);

    // Convert the aligned buffer locations into wasm linear memory indexes.
    // We want to skip the signal bytes when passing the r/w buffer into the wasm instance.
    let ro_index = (shared_ro as i64 - wasm_memory_base) as i32;
    let rw_index = (shared_rw as i64 - wasm_memory_base) as i32 + SIGNAL_BYTES;

    let create_context: Func<(i32, i32), i32> = instance.exports.get("create_context").unwrap();
    let wasm_context = create_context.call(ro_index, rw_index)
        .expect("create_context should return a context pointer");

    Wrapper { buffers, wasm_context, wasm_memory_base }
}

fn get_wasm_memory_base(instance: &Instance) -> i64 {
    let memory: Memory = instance.exports.get("memory").unwrap();
    memory.view::<u8>().as_ptr() as i64
}
