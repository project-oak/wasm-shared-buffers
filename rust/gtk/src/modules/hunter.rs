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

use common::module_common::{move_by, print_str, srand, Context, GRID_H, GRID_W};
use common::println;
use common::shared::{cptr, State};

#[no_mangle]
pub extern "C" fn malloc_(size: usize) -> cptr {
    let vec: Vec<u8> = Vec::with_capacity(size);
    let ptr = vec.as_ptr();
    std::mem::forget(vec); // Leak the vector
    ptr as cptr
}

#[no_mangle]
pub extern "C" fn create_context(ro_ptr: cptr, rw_ptr: cptr) -> *const Context {
    Context::new_unowned(ro_ptr, rw_ptr)
}

#[no_mangle]
pub extern "C" fn update_context(ctx: &mut Context, ro_ptr: cptr, rw_ptr: cptr) {
    ctx.update(ro_ptr, rw_ptr);
}

#[no_mangle]
pub extern "C" fn init(ctx: &mut Context, rand_seed: i32) {
    srand(rand_seed as usize);
    ctx.hunter.x = GRID_W / 2;
    ctx.hunter.y = GRID_H / 2;
}

#[no_mangle]
pub extern "C" fn tick(ctx: &mut Context) {
    // Find the closest runner and move towards it.
    let mut min_dx: i32 = 0;
    let mut min_dy: i32 = 0;
    let mut min_dist = 99999;
    for r in &*ctx.runners {
        if r.state == State::Dead {
            continue;
        }
        let dx: i32 = r.x as i32 - ctx.hunter.x as i32;
        let dy: i32 = r.y as i32 - ctx.hunter.y as i32;
        let dist = dx * dx + dy * dy;
        if dist < min_dist {
            min_dx = dx;
            min_dy = dy;
            min_dist = dist;
        }
    }
    move_by(&ctx.grid, &mut ctx.hunter.x, &mut ctx.hunter.y, min_dx, min_dy);
}

#[no_mangle]
pub extern "C" fn large_alloc() {
    println!("[h] Requesting large allocation");
    std::mem::forget(Vec::<u8>::with_capacity(100000));
}

#[no_mangle]
pub extern "C" fn modify_grid(ctx: &mut Context) {
    println!("[h] Attempting to write to read-only memory...");
    ctx.grid[0][0] = 2;
}

fn main() {
    println!("hunter: Not meant to be run as a main");
}
