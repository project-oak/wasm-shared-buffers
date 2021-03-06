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

// Inlined via #include in hunter.c and runner.c

EM_JS(void, print_callback, (int, const char *msg), {})
extern void print_callback(int len, const char *msg);

EMSCRIPTEN_KEEPALIVE
void print(const char *fmt, ...) {
  char msg[500];
  va_list ap;
  va_start(ap, fmt);
  int len = vsnprintf(msg, 500, fmt, ap);
  va_end(ap);
  print_callback(len, msg);
}

EMSCRIPTEN_KEEPALIVE
void *malloc_(size_t size) {
  return malloc(size);
}

EMSCRIPTEN_KEEPALIVE
void update_context(Context *ctx, void *ro_ptr, void *rw_ptr) {
  ctx->grid = ro_ptr;
  ctx->hunter = rw_ptr;
  ctx->runners = rw_ptr + sizeof(Hunter);
}

EMSCRIPTEN_KEEPALIVE
Context *create_context(void *ro_ptr, void *rw_ptr) {
  Context *ctx = malloc(sizeof(Context));
  update_context(ctx, ro_ptr, rw_ptr);
  return ctx;
}

EMSCRIPTEN_KEEPALIVE
void large_alloc() {
  // Not implemented.
}

int rand_step() {
  return (rand() % 3) - 1;
}

void move(Context *ctx, int *x, int *y, int mx, int my) {
  // If the dest cell is blocked, try a random move; if that's also blocked just stay still.
  int tx = *x + mx;
  int ty = *y + my;
  if (ctx->grid[ty][tx] == 1) {
    tx = *x + rand_step();
    ty = *y + rand_step();
    if (ctx->grid[ty][tx] == 1) {
      return;
    }
  }
  *x = tx;
  *y = ty;
}

// Converts an arbitrary delta into a unit step.
int step(int delta) {
  return (delta == 0) ? 0 : ((delta > 0) ? 1 : -1);
}
