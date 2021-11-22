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
use common::host_common::{Buffers, Comms, Signal};
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
                print!("{}", msg.get_utf8_string(ctx.memory(0), len).unwrap());
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
    let memory: Memory =  instance.exports.get("memory").unwrap();
    let get_wasm_memory_base = || memory.view::<u8>().as_ptr() as i64;

    let malloc = |size: i32| -> i32 {
        let wasm_malloc: Func<i32, i32> = instance.exports.get("malloc_").unwrap();
        wasm_malloc.call(size).expect("malloc_ failed")
    };

    let set_shared = |ro_index:i32, ro_size:i32, rw_index:i32, rw_size: i32| {
        let wasm_set_shared: Func<(i32, i32, i32, i32)> = instance.exports.get("set_shared").unwrap();
        wasm_set_shared.call(ro_index, ro_size, rw_index, rw_size).expect("set_shared failed");
    };

    Buffers::new(get_wasm_memory_base, malloc, set_shared)
}
