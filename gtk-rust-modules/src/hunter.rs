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

// #[macro_use]
// extern crate lazy_static;
mod common;
mod module_common;
// use std::sync::{Arc, Mutex};

use common::*;
use module_common::*;

#[no_mangle]
pub extern fn set_shared(ro_ptr: cptr, _ro_len: i32, rw_ptr: cptr, _rw_len: i32) {
  let mut guard = CTX.lock().expect("Failed to aquire ctx lock");
  let ctx = &mut (*guard);
  unsafe {
    ctx.replace(
      Context {
        grid: Box::from_raw(ro_ptr as *mut GridType),
        hunter: Box::from_raw(rw_ptr as *mut Hunter),
        runners: Box::from_raw(rw_ptr.add(std::mem::size_of::<Hunter>()) as *mut [Runner; N_RUNNERS]),
      }
    );
  }
}

#[no_mangle]
pub extern fn init(_rand_seed: i32) {
  let mut guard = CTX.lock().expect("Failed to aquire ctx lock");
  let ctx: &mut Context = (guard.as_mut()).expect("ctx not initialized");
  // ctx.rng = rand::thread_rng().fill(rand_seed); // Something like this?
  ctx.hunter.x = GRID_W / 2;
  ctx.hunter.y = GRID_H / 2;
}

#[no_mangle]
pub extern fn tick() {
  let mut guard = CTX.lock().expect("Failed to aquire ctx lock");
  let ctx = (*guard).as_mut().expect("ctx not initialized");
  // Find the closest runner and move towards it.
  let mut min_dx: i32 = 0;
  let mut min_dy: i32 = 0;
  let mut min_dist = 99999;
  for r in &*ctx.runners {
    if r.state == State::Dead {
      continue;
    }
    let dx: i32 = r.x as i32 - (*ctx.hunter).x as i32;
    let dy: i32 = r.y as i32 - (*ctx.hunter).y as i32;
    let dist = dx * dx + dy * dy;
    if dist < min_dist {
      min_dx = dx;
      min_dy = dy;
      min_dist = dist;
    }
  }
  move_by(&mut ctx.hunter.x, &mut ctx.hunter.y, step(min_dx), step(min_dy));
}

#[no_mangle]
pub extern fn modify_grid() {
  print!("[h] Attempting to write to read-only memory...\n");
  let mut guard = CTX.lock().expect("Failed to aquire ctx lock");
  let ctx = (*guard).as_mut().expect("ctx not initialized");
  ctx.grid[0][0] = 2;
}

fn main() {
  println!("hunter: No meant to be run as a main");
}
