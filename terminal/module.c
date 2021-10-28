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
#include <stdlib.h>
#include <string.h>
#include <emscripten.h>

const unsigned char *ro_buf = NULL;
unsigned char *rw_buf = NULL;
int ro_size = 0;
int rw_size = 0;

EMSCRIPTEN_KEEPALIVE
void set_shared(void *ro_ptr, int ro_len, void *rw_ptr, int rw_len) {
  ro_buf  = ro_ptr;
  ro_size = ro_len;
  rw_buf  = rw_ptr;
  rw_size = rw_len;
}

int scan_shared(const unsigned char *buf, int size, const char *prefix) {
  if (memcmp(buf, prefix, 3) != 0)
    return 1;
  if (memcmp(buf + size - 3, "buf", 3) != 0)
    return 2;
  unsigned char v[2] = { 131, 173 };
  for (int i = 3; i < size - 3; i++) {
    if (buf[i] != v[i % 2])
      return i;
  }
  return 0;
}

EMSCRIPTEN_KEEPALIVE
int verify_shared() {
  int res = scan_shared(ro_buf, ro_size, "ro:");
  if (res == 0) {
    res = scan_shared(rw_buf, rw_size, "rw:");
  }
  return res;
}

typedef struct Block Block;

struct Block {
  void *ptr;
  Block *next;
};

Block *fill_list = NULL;

EMSCRIPTEN_KEEPALIVE
int fill_memory() {
  for (int i = 1; i < 100; i++) {
    Block *b = malloc(sizeof(Block));
    if (b == NULL)
      return i;
    b->next = fill_list;
    fill_list = b;
    b->ptr = malloc(1000);
    if (b->ptr == NULL)
      return i;
    memset(b->ptr, 181, 1000);
  }
  return 0;
}

EMSCRIPTEN_KEEPALIVE
void clear_memory() {
  while (fill_list != NULL) {
    Block *next = fill_list->next;
    memset(fill_list->ptr, 0, 1000);
    free(fill_list->ptr);
    memset(fill_list, 0, sizeof(Block));
    free(fill_list);
    fill_list = next;
  }
}

EMSCRIPTEN_KEEPALIVE
void write_rw(int pos, unsigned char val, int len) {
  for (unsigned char *p = rw_buf + pos; len > 0; len--) {
    *p++ = val++;
  }
}

EMSCRIPTEN_KEEPALIVE
int read_rw(int pos, unsigned char val, int len) {
  for (unsigned char *p = rw_buf + pos; len > 0; len--) {
    if (*p++ != val++)
      return 1;
  }
  return 0;
}

EMSCRIPTEN_KEEPALIVE
void write_ro() {
  // Should crash the process due to virtual memory read-only protection.
  *(unsigned char *)ro_buf = 'X';
}

EMSCRIPTEN_KEEPALIVE
int force_error() {
  return *(int *)0xffffffffff;
}
