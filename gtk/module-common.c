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

EM_JS(void, print_callback, (const char* msg), {})
extern void print_callback(const char* msg);

EMSCRIPTEN_KEEPALIVE
void print(const char *fmt, ...) {
  char msg[500];
  va_list ap;
  va_start(ap, fmt);
  vsnprintf(msg, 500, fmt, ap);
  va_end(ap);
  print_callback(msg);
}

const int dirs[3] = { -1, 0, 1 };

int rand_dir() {
  return dirs[rand() % 3];
}

void move(int *x, int *y, int mx, int my) {
  // If the dest cell is blocked, try a random move; if that's also blocked just stay still.
  int tx = *x + mx;
  int ty = *y + my;
  if (grid[ty][tx] == 1) {
    tx = *x + rand_dir();
    ty = *y + rand_dir();
    if (grid[ty][tx] == 1) {
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
