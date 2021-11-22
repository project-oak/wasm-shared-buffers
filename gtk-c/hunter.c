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
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <emscripten.h>
#include "common.h"

int (*grid)[GRID_W];
Hunter *hunter;
const Runner *runners;

#include "module-common.c"

EMSCRIPTEN_KEEPALIVE
void set_shared(void *ro_ptr, int ro_len, void *rw_ptr, int rw_len) {
  grid = ro_ptr;
  hunter = rw_ptr;
  runners = rw_ptr + sizeof(Hunter);
}

EMSCRIPTEN_KEEPALIVE
void init(int rand_seed) {
  srand(rand_seed);
  hunter->x = GRID_W / 2;
  hunter->y = GRID_H / 2;
}

EMSCRIPTEN_KEEPALIVE
void tick() {
  // Find the closest runner and move towards it.
  int min_dx = 0;
  int min_dy = 0;
  int min_dist = 99999;
  const Runner *r = runners;
  for (int i = 0; i < N_RUNNERS; r++, i++) {
    if (r->state == DEAD)
      continue;
    int dx = r->x - hunter->x;
    int dy = r->y - hunter->y;
    int dist = dx * dx + dy * dy;
    if (dist < min_dist) {
      min_dx = dx;
      min_dy = dy;
      min_dist = dist;
    }
  }
  move(&hunter->x, &hunter->y, step(min_dx), step(min_dy));
}

EMSCRIPTEN_KEEPALIVE
void modify_grid() {
  print("[h] Attempting to write to read-only memory...\n");
  grid[0][0] = 2;
}
