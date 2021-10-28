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
#ifndef COMMON_H
#define COMMON_H

#define GRID_W      50
#define GRID_H      30
#define N_BLOCKS    150
#define N_RUNNERS   15
#define SCARE_DIST  10

#define SCALE       20
#define TICK_MS     200

typedef enum {
  WALKING,
  RUNNING,
  DEAD
} State;

typedef struct {
  int x;
  int y;
  State state;
} Runner;

typedef struct {
  int x;
  int y;
} Hunter;

typedef enum {
  CMD_READY = '@',
  CMD_FAILED = '*',
  CMD_INIT = 'i',
  CMD_TICK = 't',
  CMD_EXIT = 'x',
  CMD_MODIFY_GRID = 'm'
} Command;

#endif
