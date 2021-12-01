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
use std::{cell::RefCell, env, fs, io::prelude::*, process};
use wasmi::RuntimeValue::I32;

fn main() {
    let module_name = env::args().nth(1).expect("missing module name arg");
    let signal_index = env::args().nth(2).expect("missing signal index arg");
    println!("Container {} started (wasmi); module '{}', pid {}", signal_index, module_name, process::id());

    // Load and instantiate the wasm module.
    let mut bytes = Vec::new();
    fs::File::open(module_name).unwrap().read_to_end(&mut bytes).unwrap();
    let module = wasmi::Module::from_buffer(&bytes).expect("failed to load wasm");

    let mut ctx = Context::new();
    let imports = wasmi::ImportsBuilder::new().with_resolver("env", &ctx);
    let instance = wasmi::ModuleInstance::new(&module, &imports)
        .expect("failed to instantiate wasm module")
        .assert_no_start();

    // Extract the linear memory object.
    let mem_extern = instance
        .export_by_name("memory")
        .expect("module does not export memory");
    ctx.memory_ref.replace(mem_extern.as_memory().unwrap().clone());

    // Set up our shared memory buffers.
    ctx.map_shared_buffers(&instance);

    // Command loop. Comms does *not* take ownership of shared_rw.
    let mut comms = Comms::new(ctx.buffers().shared_rw, signal_index.parse().unwrap());
    loop {
        match comms.wait_for_signal() {
            Signal::Idle => panic!("unexpected idle signal"),
            Signal::Init => ctx.wasm_call(&instance, "init", &[I32(rand::random::<i32>())]),
            Signal::Tick => ctx.wasm_call(&instance, "tick", &[]),
            Signal::ModifyGrid => ctx.wasm_call(&instance, "modify_grid", &[]),
            Signal::Exit => break,
        };
        comms.send_idle();
    }
    println!("Container {} stopping", signal_index);
}

struct Context {
    memory_ref: Option<wasmi::MemoryRef>,
    buffers_ref: Option<Buffers>,
}

impl Context {
    const PRINT_CALLBACK: usize = 0;

    fn new() -> Self {
        Self {
            memory_ref: None,
            buffers_ref: None,
        }
    }

    fn memory(&self) -> &wasmi::MemoryRef {
        self.memory_ref.as_ref().unwrap()
    }

    fn buffers(&self) -> &Buffers {
        self.buffers_ref.as_ref().unwrap()
    }

    fn wasm_call(&mut self, instance: &wasmi::ModuleRef, name: &str, args: &[wasmi::RuntimeValue]) {
        instance
            .invoke_export(name, args, self)
            .unwrap_or_else(|_| panic!("wasm call '{}' failed", name));
    }

    fn map_shared_buffers(&mut self, instance: &wasmi::ModuleRef) {
        let cell = RefCell::new(self);

        let get_memory_base = || -> i64 {
            cell.borrow().memory().with_direct_access(|buf| buf.as_ptr() as i64)
        };

        let malloc = |size: i32| -> i32 {
            let wasm_alloc_res = instance.invoke_export("malloc_", &[I32(size)], *cell.borrow_mut())
                .expect("malloc_ failed")
                .expect("no value returned from malloc_");
            match wasm_alloc_res {
                I32(v) => v,
                _ => panic!("invalid value type returned from malloc_"),
            }
        };

        let set_shared = |ro_index:i32, ro_size:i32, rw_index:i32, rw_size: i32| {
            instance.invoke_export(
                "set_shared",
                &[I32(ro_index), I32(ro_size), I32(rw_index), I32(rw_size)],
                *cell.borrow_mut()
                ).expect("set_shared failed");
        };

        let buffers = Buffers::new(get_memory_base, malloc, set_shared);
        cell.borrow_mut().buffers_ref.replace(buffers);
    }
}

impl wasmi::Externals for Context {
    fn invoke_index(&mut self, index: usize, args: wasmi::RuntimeArgs) -> Result<Option<wasmi::RuntimeValue>, wasmi::Trap> {
        if index == Self::PRINT_CALLBACK {
            let len = args.nth::<i32>(0) as usize;
            let ptr = args.nth::<u32>(1);
            let buf = self.memory().get(ptr, len).unwrap();
            print!("{}", String::from_utf8(buf).unwrap());
            Ok(None)
        } else {
            panic!("unimplemented function at {}", index);
        }
    }
}

impl wasmi::ModuleImportResolver for Context {
    fn resolve_func(&self, field_name: &str, signature: &wasmi::Signature) -> Result<wasmi::FuncRef, wasmi::Error> {
        if field_name == "print_callback" {
            Ok(wasmi::FuncInstance::alloc_host(signature.clone(), Self::PRINT_CALLBACK))
        } else {
            panic!("unexpected export {}", field_name);
        }
    }
}
