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

use rand::{Rng, prelude::ThreadRng};
use crate::common::*;
use std::sync::{Arc, Mutex};

extern "C" {
  fn print_callback(len: usize, msg: *const u8); // len should be usize
}

pub fn print_str(s: &str) {
  unsafe {
    print_callback(s.len(), s.as_ptr());
  }
}

#[macro_export]
macro_rules! print {
  ($fmt:expr $(, $value:expr)* ) => {
      print_str(&format!($fmt $(, $value)*));
  };
}

pub type GridType = [[i32; GRID_H]; GRID_W];

pub struct Context {
  pub grid: Box<GridType>,
  pub hunter: Box<Hunter>,
  pub runners: Box<[Runner; N_RUNNERS]>,
}

use lazy_static::lazy_static;
lazy_static! {
    pub static ref CTX: Arc<Mutex<Option<Context>>> = Arc::new(Mutex::new(None));
}

pub fn rand_step() -> i32 {
  // let guard = CTX.lock().expect("Failed to aquire ctx lock");
  // let ctx = *guard.as_ref().expect("ctx not initialized");
  let mut rng: ThreadRng = rand::thread_rng(); //TODO: Store this globally?
  return (rng.gen::<i32>() % 3) - 1;
}

pub fn move_by(x: &mut usize, y: &mut usize, mx: i32, my: i32) {
  let guard = CTX.lock().expect("Failed to aquire ctx lock");
  let ctx = &*guard.as_ref().expect("ctx not initialized");
  // If the dest cell is blocked, try a random move;
  // if that's also blocked just stay still.
  let mut tx: usize = (*x as i64).wrapping_add(mx as i64) as usize;
  let mut ty: usize = (*y as i64).wrapping_add(my as i64) as usize;
  if (*ctx.grid)[ty][tx] == 1 {
    // TODO: This is a bit cursed
    tx = (*x as i64).checked_add(rand_step() as i64).expect("Overflow on x!?") as usize;
    ty = (*y as i64).checked_add(rand_step() as i64).expect("Overflow on y!?") as usize;
    if (*ctx.grid)[ty][tx] == 1 {
      return;
    }
  }
  *x = tx;
  *y = ty;
}

// Converts an arbitrary delta into a unit step.
pub fn step(delta: i32) -> i32 {
  if delta == 0 {
    0
  } else if delta > 0 {
    1
  } else {
    -1
  }
}
