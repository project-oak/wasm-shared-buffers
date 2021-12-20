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
  Hunter *hunter;
  const Runner *runners;
} Context;

#include "module-common.c"

EMSCRIPTEN_KEEPALIVE
void init(Context *ctx, int rand_seed) {
  srand(rand_seed);
  ctx->hunter->x = GRID_W / 2;
  ctx->hunter->y = GRID_H / 2;
}

EMSCRIPTEN_KEEPALIVE
void tick(Context *ctx) {
  // Find the closest runner and move towards it.
  int min_dx = 0;
  int min_dy = 0;
  int min_dist = 99999;
  const Runner *r = ctx->runners;
  for (int i = 0; i < N_RUNNERS; r++, i++) {
    if (r->state == DEAD)
      continue;
    int dx = r->x - ctx->hunter->x;
    int dy = r->y - ctx->hunter->y;
    int dist = dx * dx + dy * dy;
    if (dist < min_dist) {
      min_dx = dx;
      min_dy = dy;
      min_dist = dist;
    }
  }
  move(ctx, &ctx->hunter->x, &ctx->hunter->y, step(min_dx), step(min_dy));
}

EMSCRIPTEN_KEEPALIVE
void large_alloc(Context *ctx) {
  // Not implemented.
}

EMSCRIPTEN_KEEPALIVE
void modify_grid(Context *ctx) {
  print("[h] Attempting to write to read-only memory...\n");
  ctx->grid[0][0] = 2;
}
