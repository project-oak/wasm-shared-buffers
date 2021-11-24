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

// Shared buffer config.
pub const READ_ONLY_BUF_NAME: &str = "/shared_ro";
pub const READ_WRITE_BUF_NAME: &str = "/shared_rw";
pub const READ_ONLY_BUF_SIZE: usize = GRID_W * GRID_H * 4;
// 2 * i32 for signals, 2 * i32 for hunter, N * 3 * i32 for runners
pub const READ_WRITE_BUF_SIZE: usize = SIGNAL_BYTES + 8 + N_RUNNERS * 12;

// IPC config.
pub const SIGNAL_BYTES: usize = 8;
pub const HUNTER_SIGNAL_INDEX: usize = 0;
pub const RUNNER_SIGNAL_INDEX: usize = 1;
pub const SIGNAL_REPS: i32 = 100;
pub const SIGNAL_WAIT: u64 = 10;

// Grid setup.
pub const GRID_W: usize = 50;
pub const GRID_H: usize = 30;
pub const N_BLOCKS: usize = 150;
pub const N_RUNNERS: usize = 15;

// GUI settings.
pub const SCALE: f64 = 20.0;
pub const TICK_MS: u64 = 150;

#[allow(non_camel_case_types)]
pub type cptr = *mut core::ffi::c_void;

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


#[derive(Eq, PartialEq)]
pub enum State {
  Walking,
  Running,
  Dead
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

enum Command {
  Ready,
  Failed,
  Init,
  Tick,
  Exit,
  ModifyGrid,
}

impl Command {
  fn from_char(ch: char) -> Self {
    use Command::*;
    match ch {
      '@' => Ready,
      '*' => Failed,
      'i' => Init,
      't' => Tick,
      'x' => Exit,
      'm' => ModifyGrid,
       _ => panic!("Unknown character {}", ch),
    }
  }
}
