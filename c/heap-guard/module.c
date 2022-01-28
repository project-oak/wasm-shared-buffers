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

// Simulates a buffer overflow attack by scanning from the end of the stack-allocated
// 'buf' to find and overwrite the heap-allocated string set up in main().
void stack_attack() {
  char buf[10];
  for (int i = 10; i < 100000; i++) {
    if (buf[i] == 'h') {
      strcpy(&buf[i], "~HACKED~");
      return;
    }
  }
}

EMSCRIPTEN_KEEPALIVE
void test_overflow_attack() {
  // If the container hasn't been set up with the heap guard, this will be the first
  // allocation and will thus be placed at the start of the heap, easily accessible
  // via stack overflow. If the heap guard has been set up, a protected memory region
  // will be in place between the stack and this allocation.
  char *data = malloc(20);
  strcpy(data, "hello world");
  print("before: %s\n", data);
  stack_attack();
  print("after: %s\n", data);
}
