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
use std::{collections::hash_map::DefaultHasher, hash::Hasher, mem, slice};

const SUCCESS: i32 = 0;
const BUFFER_TOO_SMALL: i32 = 1;
const NOT_FOUND: i32 = 2;

extern "C" {
    fn print_callback(len: u32, msg: *const u8);
    fn lookup_callback(key_len: u32, key: *const u8, value_len: *mut u32, value: *mut u8) -> i32;
}

fn print_str(s: &str) {
    unsafe { print_callback(s.len() as u32, s.as_ptr()); }
}

#[macro_export]
macro_rules! print {
    ($fmt:expr $(, $value:expr)* ) => {
        let s = format!($fmt $(, $value)*);
        print_str(&s);
    };
}

#[macro_export]
macro_rules! println {
    ($fmt:expr $(, $value:expr)* ) => {
        let s = format!($fmt $(, $value)*) + "\n";
        print_str(&s);
    };
}

#[no_mangle]
pub extern "C" fn malloc_(size: usize) -> *const u8 {
    let vec: Vec<u8> = Vec::with_capacity(size);
    let ptr = vec.as_ptr();
    mem::forget(vec);
    ptr
}

pub struct Context {
    index: &'static [u32],
    lookup: *const u8,
    lookup_bytes: usize,
    test_keys: Vec<&'static str>,
    default_msg_bytes: u32,
}

#[no_mangle]
pub extern "C" fn create_context(
    buffer: *const u8,
    index_slots: i32,
    lookup_bytes: i32,
    num_test_keys: i32,
    test_keys_ptr: *const u8,
    test_keys_bytes: i32,
    default_msg_bytes: i32,
) -> *const Context {
    // Collect the keys to be used in the performance tests.
    let mut reader = Reader {
        buffer: test_keys_ptr,
        size: test_keys_bytes as usize,
        offset: 0,
    };
    let mut test_keys = Vec::<&str>::new();
    for _ in 0..num_test_keys {
        test_keys.push(reader.read_str());
    }

    // Create and release unownership of the context object.
    let slots = index_slots as usize;
    Box::into_raw(Box::new(unsafe {
        Context {
            index: slice::from_raw_parts(buffer as *const u32, slots),
            lookup: buffer.add(slots * 4) as *const u8,
            lookup_bytes: lookup_bytes as usize,
            test_keys,
            default_msg_bytes: default_msg_bytes as u32,
        }
    }))
}

// Check that the internal and external lookup functions match for a few different keys.
#[no_mangle]
pub extern "C" fn verify_lookups(ctx: &Context) {
    for key in ctx.test_keys.iter().take(10) {
        let value_int = lookup_int(ctx, key).unwrap();
        let value_ext = lookup_ext(ctx, key).unwrap();
        assert_eq!(value_int, value_ext);
    }
    let key = "404 not found";
    assert!(lookup_int(ctx, key).is_none());
    assert!(lookup_ext(ctx, key).is_none());
}

#[no_mangle]
pub extern "C" fn performance_test_internal(ctx: &Context) {
    for key in &ctx.test_keys {
        assert!(lookup_int(ctx, key).is_some());
    }
}

#[no_mangle]
pub extern "C" fn performance_test_external(ctx: &Context) {
    for key in &ctx.test_keys {
        assert!(lookup_ext(ctx, key).is_some());
    }
}

// Uses the "internal" mapped buffer to find the value associated with 'key'.
fn lookup_int(ctx: &Context, key: &str) -> Option<&'static str> {
    // Find the key's position in the index table..
    let mut hasher = DefaultHasher::new();
    hasher.write(key.as_bytes());
    let i = (hasher.finish() as usize) % ctx.index.len();

    // ..to get the offest into the packed data following the table.
    let offset = ctx.index[i] as usize;
    if offset > 0 {
        let mut reader = Reader {
            buffer: ctx.lookup,
            size: ctx.lookup_bytes,
            offset,
        };

        // The entry starts with the number of key/value pairs for this chain.
        let n_items = reader.read_u32();
        for _ in 0..n_items {
            // If the current key matches, extract and return the value.
            if reader.check_key(key) {
                return Some(reader.read_str());
            }

            // Otherwise, no need to read the value; skip to the next pair in the chain.
            reader.skip_str();
        }
    }
    None
}

// Calls out to the wasm host to find the value associated with 'key'.
fn lookup_ext(ctx: &Context, key: &str) -> Option<String> {
    // We start with a small size for the 'value' parameter. The host will store the result size
    // in the 'value_len' parameter, so if the initial size is too small we can update and retry.
    let mut capacity = ctx.default_msg_bytes;
    for _ in 0..2 {
        let mut value_len = Box::new(capacity);
        let mut value = Vec::with_capacity(capacity as usize);
        let res = unsafe {
            lookup_callback(key.len() as u32, key.as_ptr(), &mut *value_len, value.as_mut_ptr())
        };
        match res {
            SUCCESS => {
                // Convert the raw bytes in 'value' to a string and transfer ownership of them.
                let s = unsafe {
                    String::from_raw_parts(value.as_mut_ptr(), *value_len as usize, capacity as usize)
                };
                mem::forget(value);
                return Some(s);
            }
            BUFFER_TOO_SMALL => capacity = *value_len,
            NOT_FOUND => return None,
            _ => panic!("invalid lookup return code: {}", res),
        };
    }
    // Should never be reached.
    panic!("lookup failed");
}

// Given a buffer base pointer and starting offset, this can decode u32 and packed
// String values (u32 length followed by bytes) while advancing the offset.
struct Reader {
    buffer: *const u8,
    size: usize,
    offset: usize,
}

impl Reader {
    fn read_u32(&mut self) -> u32 {
        assert!(self.offset + 4 <= self.size);
        let res = unsafe { *mem::transmute::<_, &u32>(self.buffer.add(self.offset)) };
        self.offset += 4;
        res
    }

    fn read_str(&mut self) -> &'static str {
        let len = self.read_u32() as usize;
        assert!(self.offset + len <= self.size);
        let res = unsafe {
            let ptr = self.buffer.add(self.offset);
            let slc = slice::from_raw_parts(ptr, len);
            std::str::from_utf8_unchecked(slc)
        };
        self.offset += len;
        res
    }

    // Slightly faster key comparison using keys sorted by length.
    fn check_key(&mut self, key: &str) -> bool {
        let len = self.read_u32() as usize;
        assert!(self.offset + len <= self.size);
        if len < key.len() {
            self.offset += len;
            return false;
        }
        let res = unsafe {
            let ptr = self.buffer.add(self.offset);
            let slc = slice::from_raw_parts(ptr, len);
            std::str::from_utf8_unchecked(slc)
        };
        self.offset += len;
        res == key
    }

    fn skip_str(&mut self) {
        let len = self.read_u32() as usize;
        assert!(self.offset + len <= self.size);
        self.offset += len;
    }
}

fn main() {
    println!("reader: Not meant to be run as a main");
}
