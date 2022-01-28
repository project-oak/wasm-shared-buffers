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
#include <fcntl.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>
#include <sys/mman.h>
#include "wasm_c_api.h"
#include "common.h"

enum ExportFuncs {
  FN_MALLOC,
  FN_CREATE_CONTEXT,
  FN_UPDATE_CONTEXT,
  FN_INIT,
  FN_TICK,
  FN_MODIFY_GRID,
};

const char *kExportFuncNames[] = {
  "malloc_",
  "create_context",
  "update_context",
  "init",
  "tick",
  "modify_grid",
};

#include "../wamr-wrapper.c"

typedef struct {
  const char *label;

  // Host/container comms
  int read_fd;
  int write_fd;

  // Module runtime context
  int wasm_context;

  // Shared buffers
  unsigned char *ro_buf;
  const char *ro_name;
  int ro_size;

  unsigned char *rw_buf;
  const char *rw_name;
  int rw_size;
} Context;

Context ctx = { 0 };

// Aligns to next largest page boundary, unless p is already aligned.
static void *page_align(void *p, size_t page_size) {
  return (void *)((((size_t)p - 1) & ~(page_size - 1)) + page_size);
}

static void info(const char *fmt, ...) {
  char msg[500];
  va_list ap;
  va_start(ap, fmt);
  vsnprintf(msg, 500, fmt, ap);
  va_end(ap);
  printf("[%s] %s\n", ctx.label, msg);
}

static bool error(const char *fmt, ...) {
  char msg[500];
  va_list ap;
  va_start(ap, fmt);
  vsnprintf(msg, 500, fmt, ap);
  va_end(ap);
  fprintf(stderr, "[%s] >> %s\n", ctx.label, msg);
  return false;
}

static bool map_shared_buffers() {
  // Call wasm.malloc to reserve enough space for the shared buffers plus alignment concerns.
  int page_size = sysconf(_SC_PAGESIZE);
  int wasm_alloc_size = ctx.ro_size + ctx.rw_size + 3 * page_size;
  CallResult wasm_alloc_res = wasm_call(FN_MALLOC, wasm_alloc_size);
  if (!wasm_alloc_res.ok) {
    return false;
  }

  // Get the location of wasm's linear memory buffer in our address space.
  void *wasm_memory_base = wasm_memory_data(wc.memory);

  // Convert the reserve alloc's linear address to our address space.
  void *wasm_alloc_ptr = wasm_memory_base + wasm_alloc_res.val;

  // Align the shared buffers inside wasm's linear memory against our page boundaries.
  void *aligned_ro_ptr = page_align(wasm_alloc_ptr, page_size);
  void *aligned_rw_ptr = page_align(aligned_ro_ptr + ctx.ro_size, page_size);

  // Verify that our overall mmapped size will be safely contained in the wasm allocation.
  void *end = page_align(aligned_rw_ptr + ctx.rw_size, page_size);
  assert(end - wasm_alloc_ptr <= wasm_alloc_size);

  // Map read-only buffer.
  int flags = MAP_SHARED | MAP_FIXED;
  int ro_fd = shm_open(ctx.ro_name, O_RDONLY, S_IRUSR | S_IWUSR);
  if (ro_fd == -1) {
    return error("Error calling shm_open");
  }
  ctx.ro_buf = mmap(aligned_ro_ptr, ctx.ro_size, PROT_READ, flags, ro_fd, 0);
  assert(ctx.ro_buf == aligned_ro_ptr);

  // Map read-write buffer.
  int rw_fd = shm_open(ctx.rw_name, O_RDWR, S_IRUSR | S_IWUSR);
  if (rw_fd == -1) {
    return error("Error calling shm_open");
  }
  ctx.rw_buf = mmap(aligned_rw_ptr, ctx.rw_size, PROT_READ | PROT_WRITE, flags, rw_fd, 0);
  assert(ctx.rw_buf == aligned_rw_ptr);

  // We don't need the file descriptors once the buffers have been mapped.
  assert(close(rw_fd) != -1 && close(ro_fd) != -1);
  info("  read-only  buffer: %p", ctx.ro_buf);
  info("  read-write buffer: %p", ctx.rw_buf);

  // Inform the wasm module of the aligned shared buffer location in linear memory.
  int ro_index = (void *)ctx.ro_buf - wasm_memory_base;
  int rw_index = (void *)ctx.rw_buf - wasm_memory_base;
  CallResult ctx_res = wasm_call(FN_CREATE_CONTEXT, ro_index, rw_index);
  ctx.wasm_context = ctx_res.val;
  return ctx_res.ok;
}

static void destroy_context() {
  if (ctx.rw_buf != NULL) {
    assert(munmap(ctx.rw_buf, ctx.rw_size) != -1);
  }
  if (ctx.ro_buf != NULL) {
    assert(munmap(ctx.ro_buf, ctx.ro_size) != -1);
  }
}

static void send(Command code) {
  assert(write(ctx.write_fd, &code, 1) == 1);
}

static void command_loop() {
  bool ok = true;
  while (ok) {
    char cmd = '-';
    assert(read(ctx.read_fd, &cmd, 1) == 1);
    switch (cmd) {
      case CMD_INIT:
        ok = wasm_call(FN_INIT, ctx.wasm_context, time(NULL)).ok;
        break;
      case CMD_TICK:
        ok = wasm_call(FN_TICK, ctx.wasm_context).ok;
        break;
      case CMD_MODIFY_GRID:
        ok = wasm_call(FN_MODIFY_GRID, ctx.wasm_context).ok;
        break;
      case CMD_EXIT:
        send(cmd);
        return;
      default:
        ok = error("Unknown command code: '%c' (%d)", cmd, cmd);
        break;
    }
    if (ok) {
      // Send ack to host.
      send(cmd);
    } else {
      printf("Command failed: %c\n", cmd);
      send(CMD_FAILED);
    }
  }
}

int main(int argc, const char *argv[]) {
  assert(argc == 9);
  const char *module_name = argv[1];
  ctx.label = argv[2];
  ctx.read_fd = atoi(argv[3]);
  ctx.write_fd = atoi(argv[4]);
  ctx.ro_name = argv[5];
  ctx.ro_size = atoi(argv[6]);
  ctx.rw_name = argv[7];
  ctx.rw_size = atoi(argv[8]);

  info("Container started; module '%s', pid %d", module_name, getpid());
  if (init_module(module_name) && map_shared_buffers()) {
    send(CMD_READY);
    command_loop();
  }
  destroy_context();
  destroy_module();
}
