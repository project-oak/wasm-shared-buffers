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
use wasmi::{
    Externals, FuncInstance, FuncRef, ImportsBuilder, MemoryRef, Module, ModuleImportResolver,
    ModuleInstance, RuntimeArgs, RuntimeValue, RuntimeValue::I32, Signature, Trap,
};

fn main() {
    let module_name = env::args().nth(1).expect("missing module name arg");
    let index = env::args().nth(2).expect("missing index arg");
    println!("Container {} started (wasmi); module '{}', pid {}", index, module_name, process::id());

    // Load and instantiate the wasm module.
    let instance = {
        let mut bytes = Vec::new();
        fs::File::open(module_name).unwrap().read_to_end(&mut bytes).unwrap();
        let module = Module::from_buffer(&bytes).expect("failed to load wasm");
        let imports = ImportsBuilder::new().with_resolver("env", &Resolver);
        ModuleInstance::new(&module, &imports)
            .expect("failed to instantiate wasm module")
            .assert_no_start()
    };

    // Set up our shared memory buffers and create the wasm module's context object.
    let w = map_shared_buffers(&instance, index.parse().unwrap());

    // Command loop.
    loop {
        let signal = w.buffers.wait_for_signal();

        // Version 0.10.0 of wasmi changed to using a 4Gb pre-allocation for the linear buffer,
        // so resize ops just cause an update to the available length rather than a full realloc.
        // That means we don't need to remap our shared buffers, but as a precaution make sure
        // the wasm memory's base address is stable.
        if w.wasm_memory_base != get_wasm_memory_base(&instance) {
            println!("Container {}: linear buffer memory address changed!", index);
            break;
        }

        match signal {
            Signal::Idle => panic!("unexpected idle signal"),
            Signal::Init => wasm_call(&instance, "init", &[w.wasm_context, I32(rand::random::<i32>())]),
            Signal::Tick => wasm_call(&instance, "tick", &[w.wasm_context]),
            Signal::LargeAlloc => wasm_call(&instance, "large_alloc", &[w.wasm_context]),
            Signal::ModifyGrid => wasm_call(&instance, "modify_grid", &[w.wasm_context]),
            Signal::Exit => break,
        };
        w.buffers.send_idle();
    }
    println!("Container {} stopping", index);
}

struct Wrapper {
    buffers: Buffers,
    wasm_context: RuntimeValue,
    wasm_memory_base: i64,
}

fn map_shared_buffers(instance: &ModuleInstance, index: usize) -> Wrapper {
    // Call wasm.malloc to reserve enough space for the shared buffers plus alignment concerns.
    let wasm_alloc_res = wasm_call(instance, "malloc_", &[I32(WASM_ALLOC_SIZE)])
        .expect("no value returned from malloc_");
    let wasm_alloc_index = match wasm_alloc_res {
        I32(v) => v,
        _ => panic!("invalid value type returned from malloc_"),
    };

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
    let wasm_context = wasm_call(instance, "create_context", &[I32(ro_index), I32(rw_index)])
        .expect("create_context should return a context pointer");

    Wrapper { buffers, wasm_context, wasm_memory_base }
}

fn wasm_call(instance: &ModuleInstance, name: &str, args: &[RuntimeValue]) -> Option<RuntimeValue> {
    let mut externs = Externs { memory: get_linear_memory(instance) };
    instance
        .invoke_export(name, args, &mut externs)
        .unwrap_or_else(|_| panic!("wasm call '{}' failed", name))
}

fn get_wasm_memory_base(instance: &ModuleInstance) -> i64 {
    get_linear_memory(instance).with_direct_access(|buf| buf.as_ptr() as i64)
}

fn get_linear_memory(instance: &ModuleInstance) -> MemoryRef {
    let mem_extern = instance
        .export_by_name("memory")
        .expect("module does not export memory");
    mem_extern.as_memory().unwrap().clone()
}

const PRINT_CALLBACK: usize = 0;

struct Externs {
    memory: MemoryRef,
}

impl Externals for Externs {
    fn invoke_index(&mut self, index: usize, args: RuntimeArgs) -> Result<Option<RuntimeValue>, Trap> {
        match index {
            PRINT_CALLBACK => {
                let len = args.nth::<i32>(0) as usize;
                let ptr = args.nth::<u32>(1);
                let mut buf = vec![0; len];
                self.memory.get_into(ptr, &mut buf[..]).unwrap();
                print!("{}", String::from_utf8(buf).unwrap());
                Ok(None)
            }
            _ => panic!("unimplemented function at {}", index),
        }
    }
}

struct Resolver;

impl ModuleImportResolver for Resolver {
    fn resolve_func(&self, field_name: &str, signature: &Signature) -> Result<FuncRef, wasmi::Error> {
        match field_name {
            "print_callback" => Ok(FuncInstance::alloc_host(signature.clone(), PRINT_CALLBACK)),
            _ => panic!("unexpected export {}", field_name),
        }
    }
}
