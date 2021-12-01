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

#[derive(Eq, PartialEq, Clone, Copy)]
pub enum State {
    Walking,
    Running,
    Dead,
}

impl State {
    pub fn from(value: i32) -> Self {
        assert!((0..3).contains(&value));
        [Self::Walking, Self::Running, Self::Dead][value as usize]
    }
}

#[allow(non_camel_case_types)]
pub type cptr = *mut core::ffi::c_void;

