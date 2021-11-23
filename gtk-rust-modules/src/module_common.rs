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

#[macro_use]
extern crate lazy_static;

// EM_JS(void, print_callback, (int, const char* msg), {})
extern fn print_callback(len: i32, msg: &str); // len should be usize

macro_rules! print {
  ($fmt:expr $(, $value:expr)* ) => {
      print_callback(format!($fmt $(, $value)*));
  };
}

type GridType = [[i32; GRID_H]; GRID_W];

struct Context {
  grid: &GridType;
  hunter: &Hunter;
  runners: &[Runner];
}

lazy_static! {
    static ref CTX: Mutex<Option<Context>> = Mutex::new(None);
}

fn rand_step() -> i32 {
  return (rand() % 3) - 1;
}

fn move_by(x: &mut i32, y: &mut i32, mx: i32, my: i32) {
  let guard = CTX.lock().expect("Failed to aquire ctx lock");
  let ctx = *guard.as_ref().expect("ctx not initialized");
  // If the dest cell is blocked, try a random move;
  // if that's also blocked just stay still.
  let mut tx: i32 = x + mx;
  let mut ty: i32 = y + my;
  if ctx.grid[ty][tx] == 1 {
    tx = x + rand_step();
    ty = y + rand_step();
    if ctx.grid[ty][tx] == 1 {
      guard.unlock();
      return;
    }
  }
  guard.unlock();
  x = tx;
  y = ty;
}

// Converts an arbitrary delta into a unit step.
fn step(delta: i32) -> i32 {
  if delta == 0 {
    0
  } else if delta > 0 {
    1
  } else {
    -1
  }
}
