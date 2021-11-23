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
use module_common::*;
#[no_mangle]
pub extern fn set_shared(ro_ptr: cptr, ro_len: i32, rw_ptr: cptr, rw_len: i32) {
  let guard = CTX.lock().expect("Failed to aquire ctx lock");
  let mut ctx = *guard;
  ctx = Some(
    Context {
      grid = ro_ptr,
      hunter = rw_ptr,
      runners = rw_ptr + std::mem::size_of(Hunter),
    }
  )
  guard.unlock();
}

#[no_mangle]
pub extern fn init(rand_seed: i32) {
  let guard = CTX.lock().expect("Failed to aquire ctx lock");
  let mut ctx = *guard.as_mut_ref().expect("ctx not initialized");
  srand(rand_seed);
  ctx.hunter.x = GRID_W / 2;
  ctx.hunter.y = GRID_H / 2;
}

#[no_mangle]
pub extern fn tick() {
  // Find the closest runner and move towards it.
  let mut min_dx = 0;
  let mut min_dy = 0;
  let mut min_dist = 99999;
  for (r in ctx.runners()) {
    if (r.state == DEAD) {
      continue;
    }
    let dx = r.x - hunter.x;
    let dy = r.y - hunter.y;
    let dist = dx * dx + dy * dy;
    if (dist < min_dist) {
      min_dx = dx;
      min_dy = dy;
      min_dist = dist;
    }
  }
  move_by(&mut hunter.x, &mut hunter.y, step(min_dx), step(min_dy));
}

#[no_mangle]
pub extern fn modify_grid() {
  print!("[h] Attempting to write to read-only memory...\n");
  grid[0][0] = 2;
}
