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
const Hunter *hunter;
Runner *runners;

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
  for (int i = 0; i < N_RUNNERS; i++) {
    runners[i].x = 1 + rand() % (GRID_W - 2);
    runners[i].y = 1 + rand() % (GRID_H - 2);
    runners[i].state = WALKING;
  }
}

EMSCRIPTEN_KEEPALIVE
void tick() {
  Runner *r = runners;
  for (int i = 0; i < N_RUNNERS; r++, i++) {
    // If the hunter has reached us, we're dead.
    int dx = r->x - hunter->x;
    int dy = r->y - hunter->y;
    if (r->state == DEAD || (dx == 0 && dy == 0)) {
      r->state = DEAD;
      continue;
    }

    int mx;
    int my;
    int dist = dx * dx + dy * dy;
    if (dist > SCARE_DIST * SCARE_DIST) {
      // Hunter is too far away; random walk.
      r->state = WALKING;
      mx = rand_step();
      my = rand_step();
    } else {
      // Run! ..but with some randomness.
      r->state = RUNNING;
      switch (rand() % 3) {
        case 0:
          mx = step(dx);
          my = rand_step();
          break;
        case 1:
          mx = rand_step();
          my = step(dy);
          break;
        case 2:
          mx = step(dx);
          my = step(dy);
          break;
      }
    }
    move(&r->x, &r->y, mx, my);
  }
}

EMSCRIPTEN_KEEPALIVE
void modify_grid() {
  // Not implemented.
}
