//
// Copyright 2022 The Project Oak Authors
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
use argparse::{ArgumentParser, Store};
use libc::{MAP_FIXED, MAP_SHARED, O_CREAT, O_RDWR, O_TRUNC, PROT_READ, S_IRUSR, S_IWUSR};
use rand::{distributions::{Alphanumeric, Distribution, Uniform}, Rng};
use std::{
    collections::{hash_map::DefaultHasher, HashMap}, cmp, ffi::CString, fs::File,
    hash::Hasher, io::{prelude::*, SeekFrom}, mem, ops::RangeInclusive,
    os::unix::io::{AsRawFd, FromRawFd}, str, time::SystemTime,
};
use wasmi::{
    Error, Externals, FuncInstance, FuncRef, ImportsBuilder, LittleEndianConvert, MemoryRef,
    Module, ModuleImportResolver, ModuleInstance, ModuleRef, RuntimeArgs, RuntimeValue,
    RuntimeValue::I32, Signature, Trap,
};

const PAGE_SIZE: usize = 4096;
const MMAP_NAME: &str = "/lookup";
const KEY_SIZE: RangeInclusive<usize> = 5..=40;
const VAL_SIZE: RangeInclusive<usize> = 10..=200;

struct Params {
    lookup_entries: usize,
    index_slots: usize,
    test_keys: i32,
    default_msg_bytes: i32,
    module_name: String,
}

#[allow(non_camel_case_types)]
type cptr = *mut core::ffi::c_void;

fn main() {
    assert_eq!(PAGE_SIZE, unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize });

    let mut params = Params {
        lookup_entries: 1_000_000,
        index_slots: 128 * 1024,
        test_keys: 10_000,
        default_msg_bytes: 100,
        module_name: String::default(),
    };
    {
        let mut ap = ArgumentParser::new();
        ap.refer(&mut params.lookup_entries)
            .add_option(&["-e"], Store, "number of key/value entries in the lookup table");
        ap.refer(&mut params.index_slots)
            .add_option(&["-s"], Store, "number of hash slots in the lookup table");
        ap.refer(&mut params.test_keys)
            .add_option(&["-k"], Store, "number of test keys to use");
        ap.refer(&mut params.default_msg_bytes)
            .add_option(&["-m"], Store, "default size of message buffer for external lookup calls");
        ap.refer(&mut params.module_name)
            .add_argument("module_name", Store, "wasm module to run")
            .required();
        if ap.parse_args().is_err() {
            return;
        }
    }

    println!("Loading wasm module");
    let instance = load_wasm_module(&params.module_name);

    println!("Creating lookup table: {} entries, {} slots", params.lookup_entries, params.index_slots);
    let (lookup, test_keys) = create_lookup(&params);

    println!("Storing lookup table");
    let shm_file = store_lookup(&lookup, &params);

    let mut ctx = Context {
        instance: &instance,
        lookup,
        buffer: std::ptr::null_mut(),
        buffer_size: 0,
        wasm_context: I32(0),
    };

    println!("Storing test keys");
    let test_keys_index = store_test_keys(&ctx, &test_keys);

    println!("Initializing wasm module");
    initialise_wasm(&mut ctx, &params, &shm_file, test_keys_index, test_keys.len() as i32);
    wasm_call(&ctx, "verify_lookups", &[ctx.wasm_context]);

    println!("Running performance tests: {} reps", params.test_keys);
    let time = SystemTime::now();
    wasm_call(&ctx, "performance_test_internal", &[ctx.wasm_context]);
    let duration_int = time.elapsed().unwrap();
    println!("  internal: {:.2?}", duration_int);

    let time = SystemTime::now();
    wasm_call(&ctx, "performance_test_external", &[ctx.wasm_context]);
    let duration_ext = time.elapsed().unwrap();
    println!("  external: {:.2?}", duration_ext);
    println!("  speed up: {:.1}x", duration_ext.as_micros() as f32 / duration_int.as_micros() as f32);
}

struct Context<'a> {
    instance: &'a ModuleInstance,
    lookup: HashMap<String, String>,
    buffer: cptr,
    buffer_size: usize,
    wasm_context: RuntimeValue,
}

impl Drop for Context<'_> {
    fn drop(&mut self) {
        if self.buffer != std::ptr::null_mut() {
            assert!(self.buffer_size > 0);
            let cname = CString::new(MMAP_NAME).unwrap();
            unsafe {
                if libc::munmap(self.buffer, self.buffer_size) == -1 {
                    println!("munmap failed for shared_ro");
                }
                if libc::shm_unlink(cname.as_ptr()) == -1 {
                    println!("shm_unlink failed for shared_rw");
                }
            }
        }
    }
}

fn load_wasm_module(module_name: &str) -> ModuleRef {
    let mut bytes = Vec::new();
    File::open(module_name).unwrap().read_to_end(&mut bytes).unwrap();
    let module = Module::from_buffer(&bytes).expect("failed to load wasm");
    let imports = ImportsBuilder::new().with_resolver("env", &Resolver);
    ModuleInstance::new(&module, &imports)
        .expect("failed to instantiate wasm module")
        .assert_no_start()
}

fn create_lookup(params: &Params) -> (HashMap<String, String>, Vec<u8>) {
    let mut lookup = HashMap::new();
    let mut test_keys = Vec::new();
    let mut test_key_count = 0;
    let mut rng = rand::thread_rng();
    let key_dist = Uniform::<usize>::from(KEY_SIZE);
    let val_dist = Uniform::<usize>::from(VAL_SIZE);
    for _ in 0..params.lookup_entries {
        let key: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(key_dist.sample(&mut rng))
            .map(char::from)
            .collect();

        let val: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(val_dist.sample(&mut rng))
            .map(char::from)
            .collect();

        if test_key_count < params.test_keys {
            test_keys.extend((key.len() as u32).to_le_bytes());
            test_keys.extend(key.as_bytes());
            test_key_count += 1;
        }
        lookup.insert(key, val);
    }
    (lookup, test_keys)
}

// The lookup table is serialized with the following format:
//
//  | index table | bumper | packed chains |
//
// index table: list of u32 offsets into packed data (starting from end of the index table)
// bumper: a single unused byte so offsets of 0 can indicate an empty slot in the index table
// packed chains: a sequence of chains per used index slot; each chain has the format:
//
//  | n_pairs:u32 | key_len:u32 | key | value_len:u32 | value | key_len | ... |
//
// Keys are stored in ascending size order to enable a slightly faster lookup on the wasm side.
fn store_lookup(lookup: &HashMap<String, String>, params: &Params) -> File {
    // Convert the map to a table with vectors of key/value pairs.
    let mut table = Vec::<Vec<KeyValue>>::with_capacity(params.index_slots);
    table.resize(params.index_slots, Vec::new());
    for (key, val) in lookup.iter() {
        let mut hasher = DefaultHasher::new();
        hasher.write(key.as_bytes());
        let i = (hasher.finish() as usize) % params.index_slots;
        table[i].push(KeyValue(key.to_string(), val.to_string()));
    }

    // Create the shared memory file.
    let cname = CString::new(MMAP_NAME).unwrap();
    let fd = unsafe {
        libc::shm_open(cname.as_ptr(), O_CREAT | O_TRUNC | O_RDWR, S_IRUSR | S_IWUSR)
    };
    if fd == -1 {
        panic!("shm_open failed");
    }
    let mut file = unsafe { File::from_raw_fd(fd) };

    // Zero out the index table, adding a single bumper byte after it to allow indexes
    // of zero to indicate an empty slot.
    file.set_len((params.index_slots * 4 + 1) as u64).unwrap();

    // Pack the key/value pairs onto the end of the file, tracking offsets (from the
    // start of the packed region, not the file) in the index table.
    let mut offset = 1u32;
    let mut num_chains = 0usize;
    let mut sum_chain = 0usize;
    let mut max_chain = 0usize;
    for i in 0..params.index_slots {
        if table[i].len() > 0 {
            table[i].sort();
            let list = &table[i];

            // Update index table with current offset.
            file.seek(SeekFrom::Start((i * 4) as u64)).unwrap();
            write_u32(&mut file, offset);

            // Append the list of key/value pairs to the file.
            file.seek(SeekFrom::End(0)).unwrap();
            offset += write_u32(&mut file, list.len() as u32);
            for KeyValue(key, val) in list {
                let kbytes = key.as_bytes();
                offset += write_u32(&mut file, kbytes.len() as u32);
                offset += write_bytes(&mut file, kbytes);

                let vbytes = val.as_bytes();
                offset += write_u32(&mut file, vbytes.len() as u32);
                offset += write_bytes(&mut file, vbytes);
            }
            num_chains += 1;
            sum_chain += list.len();
            max_chain = cmp::max(list.len(), max_chain);
        }
    }
    file.flush().unwrap();

    println!("  size: {:.1} Mb", file.metadata().unwrap().len() as f64 / (1024.0 * 1024.0));
    println!("  avg chain: {:.1}", sum_chain as f64 / num_chains as f64);
    println!("  max chain: {}", max_chain);
    file
}

#[derive(Debug, Clone)]
struct KeyValue(String, String);

impl Ord for KeyValue {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        if self.0 < other.0 {
            cmp::Ordering::Less
        } else if self.0 > other.0 {
            cmp::Ordering::Greater
        } else {
            self.1.cmp(&other.1)
        }
    }
}

impl PartialOrd for KeyValue {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for KeyValue {}

impl PartialEq for KeyValue {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0 && self.1 == other.1
    }
}

fn write_u32(file: &mut File, num: u32) -> u32 {
    write_bytes(file, &num.to_le_bytes())
}

fn write_bytes(file: &mut File, bytes: &[u8]) -> u32 {
    file.write_all(bytes).unwrap();
    bytes.len() as u32
}

// Store the test keys as "packed strings" (u32 length followed by utf8 bytes).
fn store_test_keys(ctx: &Context, test_keys: &Vec<u8>) -> i32 {
    let alloc_index = wasm_alloc(ctx, test_keys.len() as i32);
    get_linear_memory(ctx).with_direct_access_mut(|buf| {
        let mut bi = alloc_index as usize;
        for ki in 0..test_keys.len() {
            buf[bi] = test_keys[ki];
            bi += 1;
        }
    });
    alloc_index
}

// Set up the mapped buffer and create the wasm's context object.
fn initialise_wasm(
    ctx: &mut Context,
    params: &Params,
    shm_file: &File,
    test_keys_index: i32,
    test_keys_bytes: i32,
) {
    // Call wasm.malloc to reserve enough space for the mapped buffer plus alignment concerns.
    ctx.buffer_size = shm_file.metadata().unwrap().len() as usize;
    let alloc_size = ctx.buffer_size + 2 * PAGE_SIZE;
    let wasm_alloc_index = wasm_alloc(ctx, alloc_size as i32);

    // Get the location of wasm's linear memory buffer in our address space.
    let wasm_memory_base = get_linear_memory(ctx).with_direct_access(|buf| buf.as_ptr() as usize);
    let wasm_alloc_ptr = wasm_memory_base + wasm_alloc_index as usize;

    // Align the buffer inside wasm's linear memory against our page boundaries and map it in.
    let aligned_ptr = page_align(wasm_alloc_ptr);
    ctx.buffer = unsafe {
        libc::mmap(
            aligned_ptr as cptr,
            ctx.buffer_size,
            PROT_READ,
            MAP_FIXED | MAP_SHARED,
            shm_file.as_raw_fd(),
            0,
        )
    };
    assert_eq!(ctx.buffer as usize, aligned_ptr);

    // Convert the aligned buffer location into its wasm linear memory index and inform the module.
    let wasm_buf_index = (ctx.buffer as usize - wasm_memory_base) as i32;
    let lookup_bytes = ctx.buffer_size - params.index_slots * 4;
    ctx.wasm_context = wasm_call(
        ctx,
        "create_context",
        &[
            I32(wasm_buf_index),
            I32(params.index_slots as i32),
            I32(lookup_bytes as i32),
            I32(params.test_keys),
            I32(test_keys_index),
            I32(test_keys_bytes),
            I32(params.default_msg_bytes),
        ],
    ).expect("create_context should return a context pointer");
}

fn page_align(ptr: usize) -> usize {
    ((ptr - 1) & !(PAGE_SIZE - 1)) + PAGE_SIZE
}

fn wasm_alloc(ctx: &Context, size: i32) -> i32 {
    let wasm_alloc_res = wasm_call(ctx, "malloc_", &[I32(size)])
        .expect("no value returned from malloc_");
    match wasm_alloc_res {
        I32(v) => v,
        _ => panic!("invalid value type returned from malloc_"),
    }
}

fn wasm_call(ctx: &Context, name: &str, args: &[RuntimeValue]) -> Option<RuntimeValue> {
    let mut externs = Externs {
        memory: get_linear_memory(ctx),
        lookup: &ctx.lookup,
    };
    ctx.instance
        .invoke_export(name, args, &mut externs)
        .unwrap_or_else(|_| panic!("wasm call '{}' failed", name))
}

fn get_linear_memory(ctx: &Context) -> MemoryRef {
    let mem_extern = ctx
        .instance
        .export_by_name("memory")
        .expect("module does not export memory");
    mem_extern.as_memory().unwrap().clone()
}

struct Externs<'a> {
    memory: MemoryRef,
    lookup: &'a HashMap<String, String>,
}

const PRINT_CALLBACK: usize = 0;
const LOOKUP_CALLBACK: usize = 1;

const SUCCESS: i32 = 0;
const BUFFER_TOO_SMALL: i32 = 1;
const NOT_FOUND: i32 = 2;

impl Externs<'_> {
    fn extract_str(&self, args: &RuntimeArgs, index: usize) -> String {
        let len = args.nth::<u32>(index);
        let ptr = args.nth::<u32>(index + 1);
        let mut buf = vec![0; len as usize];
        self.memory.get_into(ptr, &mut buf[..]).unwrap();
        String::from_utf8(buf).unwrap()
    }

    fn print_callback(&self, args: &RuntimeArgs) -> Result<Option<RuntimeValue>, Trap> {
        // The function signature from the wasm side is:
        //   (len: u32, msg: *const u8)
        print!("{}", self.extract_str(&args, 0));
        Ok(None)
    }

    fn lookup_callback(&self, args: &RuntimeArgs) -> Result<Option<RuntimeValue>, Trap> {
        // The function signature from the wasm side is:
        //   (key_len: u32, key: *const u8, value_len: *mut u32, value: *mut u8) -> i32
        //
        // The wasm module allocates a default size for 'value' and stores that in 'value_len'.
        let key = self.extract_str(&args, 0);
        match self.lookup.get(&key) {
            Some(result) => {
                // Read the available length of the 'value' parameter.
                let value_len_ptr = args.nth::<u32>(2);
                let mut buf = [0; 4];
                self.memory.get_into(value_len_ptr, &mut buf).unwrap();
                let value_len: u32 = unsafe { mem::transmute(buf) };

                // Store the length of the result back into the 'value_len' parameter.
                self.memory.set_value(value_len_ptr, result.len() as i32).unwrap();
                if result.len() <= value_len as usize {
                    // 'value' is large enough to hold the result; store and return.
                    let value_ptr = args.nth::<u32>(3);
                    self.memory.set_value(value_ptr, StringValue::new(result)).unwrap();
                    return Ok(Some(I32(SUCCESS)));
                } else {
                    // 'value' is too small to hold the result. The wasm module will read the
                    // actual size and retry with an upsized value parameter.
                    return Ok(Some(I32(BUFFER_TOO_SMALL)));
                }
            }
            None => {
                return Ok(Some(I32(NOT_FOUND)));
            }
        }
    }
}

impl Externals for Externs<'_> {
    fn invoke_index(&mut self, index: usize, args: RuntimeArgs) -> Result<Option<RuntimeValue>, Trap> {
        match index {
            PRINT_CALLBACK => self.print_callback(&args),
            LOOKUP_CALLBACK => self.lookup_callback(&args),
            _ => panic!("unimplemented function at {}", index),
        }
    }
}

struct Resolver;

impl ModuleImportResolver for Resolver {
    fn resolve_func(&self, field_name: &str, signature: &Signature) -> Result<FuncRef, Error> {
        let index = match field_name {
            "print_callback" => PRINT_CALLBACK,
            "lookup_callback" => LOOKUP_CALLBACK,
            _ => panic!("unexpected export {}", field_name),
        };
        Ok(FuncInstance::alloc_host(signature.clone(), index))
    }
}

// Minimal String wrapper to work with wasmi's MemoryInstance::set_value().
struct StringValue {
    val: String,
}

impl StringValue {
    fn new(val: &String) -> Self {
        Self { val: val.to_string() }
    }
}

impl Default for StringValue {
    fn default() -> Self {
        panic!("StringValue::default unused");
    }
}

impl AsRef<[u8]> for StringValue {
    fn as_ref(&self) -> &[u8] {
        self.val.as_bytes()
    }
}

impl AsMut<[u8]> for StringValue {
    fn as_mut(&mut self) -> &mut [u8] {
        panic!("StringValue::as_mut unused")
    }
}

impl LittleEndianConvert for StringValue {
    type Bytes = Self;

    fn into_le_bytes(self) -> Self::Bytes {
        self
    }

    fn from_le_bytes(_bytes: Self::Bytes) -> Self {
        panic!("StringValue::from_le_bytes unused")
    }
}
