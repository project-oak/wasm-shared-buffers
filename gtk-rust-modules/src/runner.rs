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

mod module_common;
use module_common::*;

const SCARE_DIST: i32 = 10;

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
pub extern fn malloc_(size: usize) -> cptr {
  let vec: Vec<u8> = Vec::with_capacity(size);
  let ptr = vec.as_ptr();
  std::mem::forget(vec); // Leak the vector
  ptr as cptr
}

#[no_mangle]
pub extern fn init(rand_seed: i32) {
  let mut guard = CTX.lock().expect("Failed to aquire ctx lock");
  let ctx: &mut Context = (guard.as_mut()).expect("ctx not initialized");
  // ctx.rng = rand::thread_rng().fill(rand_seed); // Something like this?
  srand(rand_seed as usize);
  for r in &mut *ctx.runners {
    r.x = 1 + rand_usize() % (GRID_W - 2);
    r.y = 1 + rand_usize() % (GRID_H - 2);
    r.state = State::Walking;
  }
}

#[no_mangle]
pub extern fn tick() {
  let mut guard = CTX.lock().expect("Failed to aquire ctx lock");
  let ctx = (*guard).as_mut().expect("ctx not initialized");
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
        n => {
          print!("FAILED {}", n);
          todo!()
        }
      }
    };
    move_by(&ctx.grid, &mut r.x, &mut r.y, mx, my);
  }
}

#[no_mangle]
<<<<<<< HEAD
  pub extern fn modify_grid() {
    print!("[h] Attempting to write to read-only memory...\n");
=======
pub extern "C" fn modify_grid() {
    println!("[h] Attempting to write to read-only memory...");
>>>>>>> 5b637ea (Fixed a typo and added a println for the modules)
    let mut guard = CTX.lock().expect("Failed to aquire ctx lock");
    let ctx = (*guard).as_mut().expect("ctx not initialized");
    ctx.grid[0][0] = 2;
  }

<<<<<<< HEAD
  fn main() {
    println!("hunter: No meant to be run as a main");
  }
=======
fn main() {
    println!("runner: No meant to be run as a main");
}
>>>>>>> 5b637ea (Fixed a typo and added a println for the modules)
