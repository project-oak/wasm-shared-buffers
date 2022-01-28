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

// Imported via `use` in hunter.rs and runner.rs

use super::shared::{cptr, State};

// Grid setup.
pub const GRID_W: usize = 50;
pub const GRID_H: usize = 30;
pub const N_RUNNERS: usize = 15;

extern "C" {
    pub fn print_callback(len: usize, msg: *const u8);
}

pub fn print_str(s: &str) {
    unsafe {
        print_callback(s.len(), s.as_ptr());
    }
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

static mut RAND_VALUE: usize = 0;
const SOME_LARGEISH_PRIME: usize = 137;
const SOME_OTHER_LARGEISH_PRIME: usize = 7;

pub fn srand(rand_seed: usize) {
    unsafe {
        RAND_VALUE = rand_seed as usize;
    }
}

pub fn rand() -> i32 {
    rand_usize() as i32
}

pub fn rand_usize() -> usize {
    unsafe {
        RAND_VALUE = (RAND_VALUE.wrapping_add(SOME_LARGEISH_PRIME)).wrapping_mul(SOME_OTHER_LARGEISH_PRIME);
        RAND_VALUE
    }
}

pub struct Runner {
    pub x: usize,
    pub y: usize,
    pub state: State,
}

pub struct Hunter {
    pub x: usize,
    pub y: usize,
}

pub type GridType = [[i32; GRID_W]; GRID_H];
pub type RunnersType = [Runner; N_RUNNERS];

pub struct Context {
    pub grid: &'static mut GridType,
    pub hunter: &'static mut Hunter,
    pub runners: &'static mut RunnersType,
}

impl Context {
    pub fn new_unowned(ro_ptr: cptr, rw_ptr: cptr) -> *mut Self {
        Box::into_raw(Box::new(unsafe {
            Context {
                grid: &mut *(ro_ptr as *mut GridType),
                hunter: &mut *(rw_ptr as *mut Hunter),
                runners: &mut *(skip_hunter(rw_ptr) as *mut RunnersType),
            }
        }))
    }

    pub fn update(&mut self, ro_ptr: cptr, rw_ptr: cptr) {
        unsafe {
            self.grid = &mut *(ro_ptr as *mut GridType);
            self.hunter = &mut *(rw_ptr as *mut Hunter);
            self.runners = &mut *(skip_hunter(rw_ptr) as *mut RunnersType);
        }
    }
}

fn skip_hunter(ptr: cptr) -> cptr {
    unsafe { ptr.add(std::mem::size_of::<Hunter>()) }
}

pub fn rand_step() -> i32 {
    (rand().abs() % 3) - 1
}

pub fn move_by(grid: &GridType, x: &mut usize, y: &mut usize, mx: i32, my: i32) {
    // If the dest cell is blocked, try a random move;
    // if that's also blocked just stay still.
    let (mx, my) = (step(mx), step(my));
    let mut tx: usize = (*x as i32).saturating_add(mx) as usize;
    let mut ty: usize = (*y as i32).saturating_add(my) as usize;
    if ty >= grid.len() || tx >= grid[ty].len() {
        return;
    }
    if grid[ty][tx] == 1 {
        tx = (*x as i32).saturating_add(rand_step()) as usize;
        ty = (*y as i32).saturating_add(rand_step()) as usize;
        if ty >= grid.len() || tx >= grid[ty].len() || grid[ty][tx] == 1 {
            return;
        }
    }
    *x = tx;
    *y = ty;
}

// Converts an arbitrary delta into a unit step.
pub fn step(delta: i32) -> i32 {
    use std::cmp::Ordering::*;
    match delta.cmp(&0) {
        Equal => 0,
        Greater => 1,
        Less => -1,
    }
}
