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

typedef struct {
  int (*grid)[GRID_W];
  const Hunter *hunter;
  Runner *runners;
} Context;

#include "module-common.c"

EMSCRIPTEN_KEEPALIVE
void init(Context *ctx, int rand_seed) {
  srand(rand_seed);
  Runner *r = ctx->runners;
  for (int i = 0; i < N_RUNNERS; r++, i++) {
    r->x = 1 + rand() % (GRID_W - 2);
    r->y = 1 + rand() % (GRID_H - 2);
    r->state = WALKING;
  }
}

EMSCRIPTEN_KEEPALIVE
void tick(Context *ctx) {
  Runner *r = ctx->runners;
  for (int i = 0; i < N_RUNNERS; r++, i++) {
    // If the hunter has reached us, we're dead.
    int dx = r->x - ctx->hunter->x;
    int dy = r->y - ctx->hunter->y;
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
    move(ctx, &r->x, &r->y, mx, my);
  }
}

EMSCRIPTEN_KEEPALIVE
void large_alloc(Context *ctx) {
  // Not implemented.
}

EMSCRIPTEN_KEEPALIVE
void modify_grid(Context *ctx) {
  // Not implemented.
}
