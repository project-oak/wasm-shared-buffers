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
use libc::{MAP_FIXED, MAP_SHARED, O_RDONLY, O_RDWR, PROT_READ, PROT_WRITE, S_IRUSR, S_IWUSR};
use std::{env, ffi::CString, fs, io::prelude::*, process, slice, thread, time::Duration};
use wasmi::RuntimeValue::I32;

fn main() {
    let module_name = env::args().nth(1).expect("missing module name arg");
    let signal_index = env::args().nth(2).expect("missing signal index arg");
    println!("Container {} started; module '{}', pid {}", signal_index, module_name, process::id());

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
    let mut comms = Comms::new(ctx.shared_rw, signal_index.parse().unwrap());
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
    shared_ro: cptr,
    shared_rw: cptr,
}

impl Context {
    const PRINT_CALLBACK: usize = 0;

    fn new() -> Self {
        Self {
            memory_ref: None,
            shared_ro: std::ptr::null_mut(),
            shared_rw: std::ptr::null_mut(),
        }
    }

    fn memory(&self) -> &wasmi::MemoryRef {
        self.memory_ref.as_ref().unwrap()
    }

    fn wasm_call(&mut self, instance: &wasmi::ModuleRef, name: &str, args: &[wasmi::RuntimeValue]) {
        instance
            .invoke_export(name, args, self)
            .expect(&format!("wasm call '{}' failed", name));
    }

    fn map_shared_buffers(&mut self, instance: &wasmi::ModuleRef) {
        // Call wasm.malloc to reserve enough space for the shared buffers plus alignment concerns.
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        let wasm_alloc_size = READ_ONLY_BUF_SIZE + READ_WRITE_BUF_SIZE + 3 * page_size as i32;

        let wasm_alloc_res = instance.invoke_export("malloc", &[I32(wasm_alloc_size)], self)
            .expect("malloc failed")
            .expect("no value returned from malloc");

        let wasm_alloc_index = match wasm_alloc_res {
            I32(v) => v,
            _ => panic!("invalid value type returned from malloc"),
        };

        // Get the location of wasm's linear memory buffer in our address space.
        let wasm_memory_base = self.memory().with_direct_access(|buf| buf.as_ptr() as i64);
        let wasm_alloc_ptr = wasm_memory_base + wasm_alloc_index as i64;

        // Align the shared buffers inside wasm's linear memory against our page boundaries.
        let aligned_ro_ptr = page_align(wasm_alloc_ptr, page_size);
        let aligned_rw_ptr = page_align(aligned_ro_ptr + READ_ONLY_BUF_SIZE as i64, page_size);

        // Map the buffers into the aligned locations.
        self.shared_ro = open_shared_buffer(aligned_ro_ptr, READ_ONLY_BUF_NAME, READ_ONLY_BUF_SIZE, true);
        self.shared_rw = open_shared_buffer(aligned_rw_ptr, READ_WRITE_BUF_NAME, READ_WRITE_BUF_SIZE, false);

        // Convert the aligned buffer locations into wasm linear memory indexes.
        // We want to skip the signal bytes when passing the r/w buffer into the wasm instance.
        let ro_index = (self.shared_ro as i64 - wasm_memory_base) as i32;
        let rw_index = (self.shared_rw as i64 - wasm_memory_base) as i32 + SIGNAL_BYTES;
        instance.invoke_export(
            "set_shared",
            &[
                I32(ro_index),
                I32(READ_ONLY_BUF_SIZE),
                I32(rw_index),
                I32(READ_WRITE_BUF_SIZE - SIGNAL_BYTES)
            ],
            self
        ).expect("set_shared failed");
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            if self.shared_ro != std::ptr::null_mut() {
                if libc::munmap(self.shared_ro, READ_ONLY_BUF_SIZE as usize) == -1 {
                    println!("munmap failed for shared_ro");
                }
            }
            if self.shared_rw != std::ptr::null_mut() {
                if libc::munmap(self.shared_rw, READ_WRITE_BUF_SIZE as usize) == -1 {
                    println!("munmap failed for shared_rw");
                }
            }
        }
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

fn open_shared_buffer(aligned_ptr: i64, name: &str, size: i32, read_only: bool) -> cptr {
    let cname = CString::new(name).unwrap();
    let (open_flags, map_flags) = match read_only {
        false => (O_RDWR, PROT_READ | PROT_WRITE),
        true => (O_RDONLY, PROT_READ),
    };
    unsafe {
        let fd = libc::shm_open(cname.as_ptr(), open_flags, S_IRUSR | S_IWUSR);
        if fd == -1 {
            panic!("shm_open failed for {}", name);
        }
        let buf = libc::mmap(aligned_ptr as cptr, size as usize, map_flags, MAP_FIXED | MAP_SHARED, fd, 0);
        assert!(buf == aligned_ptr as cptr);
        if libc::close(fd) == -1 {
            panic!("close failed for {}", name);
        }
        buf
    }
}

// Aligns to next largest page boundary, unless ptr is already aligned.
fn page_align(ptr: i64, page_size: i64) -> i64 {
    ((ptr - 1) & !(page_size - 1)) + page_size
}

// Wraps the (unowned) read-write buffer to provide polling IPC between the host and containers.
struct Comms<'a> {
    data: &'a mut [i32],
    index: usize,
}

impl Comms<'_> {
    fn new(shared_rw: cptr, index: usize) -> Self {
        assert!(index == HUNTER_SIGNAL_INDEX || index == RUNNER_SIGNAL_INDEX);
        Self {
            data: unsafe { slice::from_raw_parts_mut(shared_rw as *mut i32, SIGNAL_BYTES as usize / 4) },
            index,
        }
    }

    fn wait_for_signal(&self) -> Signal {
        for _ in 0..SIGNAL_REPS {
            let signal = Signal::from(self.data[self.index]);
            if signal != Signal::Idle {
                return signal;
            }
            thread::sleep(Duration::from_millis(SIGNAL_WAIT));
        }
        panic!("container {} failed to received signal", self.index);
    }

    fn send_idle(&mut self) {
        self.data[self.index] = Signal::Idle as i32;
    }
}
