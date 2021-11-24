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
use libc::{MAP_FIXED, MAP_SHARED, O_RDONLY, O_RDWR, PROT_READ, PROT_WRITE, S_IRUSR, S_IWUSR};
use std::{ffi::CString, slice, thread, time::Duration};

// Shared buffer config.
pub const READ_ONLY_BUF_NAME: &str = "/shared_ro";
pub const READ_WRITE_BUF_NAME: &str = "/shared_rw";
pub const READ_ONLY_BUF_SIZE: i32 = GRID_W * GRID_H * 4;
// 2 * i32 for signals, 2 * i32 for hunter, N * 3 * i32 for runners
pub const READ_WRITE_BUF_SIZE: i32 = SIGNAL_BYTES + 8 + N_RUNNERS * 12;

// IPC config.
pub const SIGNAL_BYTES: i32 = 8;
pub const HUNTER_SIGNAL_INDEX: usize = 0;
pub const RUNNER_SIGNAL_INDEX: usize = 1;
pub const SIGNAL_REPS: i32 = 300;
pub const SIGNAL_WAIT: u64 = 10;

// Grid setup.
pub const GRID_W: i32 = 50;
pub const GRID_H: i32 = 30;
pub const N_BLOCKS: i32 = 150;
pub const N_RUNNERS: i32 = 15;

// GUI settings.
pub const SCALE: f64 = 20.0;
pub const TICK_MS: u64 = 150;

#[allow(non_camel_case_types)]
pub type cptr = *mut core::ffi::c_void;

// -- Definitions for both host and containers --

#[derive(Copy, Clone, PartialEq)]
pub enum Signal {
    Idle,
    Init,
    Tick,
    ModifyGrid,
    Exit,
}

impl Signal {
    pub fn from(value: i32) -> Self {
        assert!(value >= 0 && value < 5);
        [Self::Idle, Self::Init, Self::Tick, Self::ModifyGrid, Self::Exit][value as usize]
    }
}

// -- Definitions for containers only --

pub struct Buffers {
    pub shared_ro: cptr,
    pub shared_rw: cptr,
}

impl Buffers {
    pub fn new(aligned_ro_ptr: i64, aligned_rw_ptr: i64) -> Self {
        Buffers {
            shared_ro: Self::map_buffer(aligned_ro_ptr, READ_ONLY_BUF_NAME, READ_ONLY_BUF_SIZE, true),
            shared_rw: Self::map_buffer(aligned_rw_ptr, READ_WRITE_BUF_NAME, READ_WRITE_BUF_SIZE, false),
        }
    }

    // Uses the libc POSIX API to map in a shared memory buffer.
    fn map_buffer(aligned_ptr: i64, name: &str, size: i32, read_only: bool) -> cptr {
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
}

impl Drop for Buffers {
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

// Aligns to next largest page boundary, unless ptr is already aligned.
pub fn page_align(ptr: i64, page_size: i64) -> i64 {
    ((ptr - 1) & !(page_size - 1)) + page_size
}

// Wraps the (unowned) read-write buffer to provide polling IPC from the host to containers.
// (The host process has its own implementation of this logic.)
pub struct Comms<'a> {
    data: &'a mut [i32],
    index: usize,
}

impl Comms<'_> {
    pub fn new(shared_rw: cptr, index: usize) -> Self {
        assert!(index == HUNTER_SIGNAL_INDEX || index == RUNNER_SIGNAL_INDEX);
        Self {
            data: unsafe { slice::from_raw_parts_mut(shared_rw as *mut i32, SIGNAL_BYTES as usize / 4) },
            index,
        }
    }

    pub fn wait_for_signal(&self) -> Signal {
        for _ in 0..SIGNAL_REPS {
            let signal = Signal::from(self.data[self.index]);
            if signal != Signal::Idle {
                return signal;
            }
            thread::sleep(Duration::from_millis(SIGNAL_WAIT));
        }
        panic!("container {} failed to received signal", self.index);
    }

    pub fn send_idle(&mut self) {
        self.data[self.index] = Signal::Idle as i32;
    }
}
