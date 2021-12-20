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

use common::module_common::{move_by, print_str, rand, rand_step, rand_usize, srand, Context, GRID_H, GRID_W};
use common::println;
use common::shared::{cptr, State};

const SCARE_DIST: i32 = 10;

#[no_mangle]
pub extern "C" fn malloc_(size: usize) -> cptr {
    let vec: Vec<u8> = Vec::with_capacity(size);
    let ptr = vec.as_ptr();
    std::mem::forget(vec); // Leak the vector
    ptr as cptr
}

#[no_mangle]
pub extern "C" fn create_context(ro_ptr: cptr, rw_ptr: cptr) -> *mut Context {
    Context::new_unowned(ro_ptr, rw_ptr)
}

#[no_mangle]
pub extern "C" fn update_context(ctx: &mut Context, ro_ptr: cptr, rw_ptr: cptr) {
    ctx.update(ro_ptr, rw_ptr);
}

#[no_mangle]
pub extern "C" fn init(ctx: &mut Context, rand_seed: i32) {
    srand(rand_seed as usize);
    for r in &mut *ctx.runners {
        r.x = 1 + rand_usize() % (GRID_W - 2);
        r.y = 1 + rand_usize() % (GRID_H - 2);
        r.state = State::Walking;
    }
}

#[no_mangle]
pub extern "C" fn tick(ctx: &mut Context) {
    // Find the closest runner and move towards it.
    for r in &mut *ctx.runners {
        if r.state == State::Dead {
            continue;
        }
        let dx: i32 = r.x as i32 - ctx.hunter.x as i32;
        let dy: i32 = r.y as i32 - ctx.hunter.y as i32;
        // If the hunter has reached us, we're dead.
        if dx == 0 && dy == 0 {
            r.state = State::Dead;
            continue;
        }

        let dist = dx * dx + dy * dy;
        let (mx, my) = if dist > SCARE_DIST * SCARE_DIST {
            // Hunter is too far away; random walk.
            r.state = State::Walking;
            (rand_step(), rand_step())
        } else {
            // Run! ..but with some randomness.
            r.state = State::Running;
            match rand().abs() % 3 {
                0 => (dx, rand_step()),
                1 => (rand_step(), dy),
                2 => (dx, dy),
                _ => return,
            }
        };
        move_by(&ctx.grid, &mut r.x, &mut r.y, mx, my);
    }
}

#[no_mangle]
pub extern "C" fn large_alloc(_ctx: &mut Context) {
    println!("[r] Requesting large allocation");
    std::mem::forget(Vec::<u8>::with_capacity(100000));
}

#[no_mangle]
pub extern "C" fn modify_grid(_ctx: &mut Context) {
    // Not implemented
}

fn main() {
    println!("runner: Not meant to be run as a main");
}
