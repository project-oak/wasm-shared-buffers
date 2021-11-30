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

// Ownership indicator as used by the wasm-c-api code.
#define own

enum ExportFuncs {
  FN_MALLOC,
  FN_SET_SHARED,
  FN_INIT,
  FN_TICK,
  FN_MODIFY_GRID,
};

const char *kExportFuncNames[] = {
  "malloc_",
  "set_shared",
  "init",
  "tick",
  "modify_grid",
};

#define N_FUNCS  (sizeof(kExportFuncNames) / sizeof(*kExportFuncNames))

typedef struct {
  const char *module_name;
  char label;
  int read_fd;
  int write_fd;

  // Engine components
  own wasm_engine_t *engine;
  own wasm_store_t *store;
  own wasm_module_t *module;
  own wasm_instance_t *instance;
  own wasm_exporttype_vec_t module_exports;
  own wasm_extern_vec_t instance_exports;

  // Export references
  wasm_memory_t *memory;
  wasm_func_t *funcs[N_FUNCS];

  // Shared buffers
  own unsigned char *ro_buf;
  const char *ro_name;
  int ro_size;

  own unsigned char *rw_buf;
  const char *rw_name;
  int rw_size;
} WasmComponents;

WasmComponents wc = { 0 };

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
  printf("[%c] %s\n", wc.label, msg);
}

static bool error(const char *fmt, ...) {
  char msg[500];
  va_list ap;
  va_start(ap, fmt);
  vsnprintf(msg, 500, fmt, ap);
  va_end(ap);
  fprintf(stderr, "[%c] >> %s\n", wc.label, msg);
  return false;
}

typedef struct {
  bool ok;
  int val;
} FuncResult;

// Wraps the cumbersome wasm_func_call API. Assumes args and return value, where present, are i32.
static FuncResult fn_call(int index, ...) {
  wasm_func_t *fn = wc.funcs[index];
  int arity = wasm_func_param_arity(fn);
  bool has_result = wasm_func_result_arity(fn);
  const char *name = kExportFuncNames[index];

  // Args vector; capacity of 10 but reset current number to 0.
  wasm_val_t args[10] = { 0 };
  wasm_val_vec_t args_vec = WASM_ARRAY_VEC(args);
  args_vec.num_elems = 0;
  assert(arity <= args_vec.size);

  // Process varags to fill in required number of elements in args/args_vec.
  va_list ap;
  va_start(ap, index);
  for (int i = 0; i < arity; i++) {
    int val = va_arg(ap, int);
    args[i].kind = WASM_I32;
    args[i].of.i32 = val;
    args_vec.num_elems++;
  }
  va_end(ap);

  // Call the wasm function.
  FuncResult result = { false, 0 };
  wasm_val_t res[1] = { WASM_INIT_VAL };
  wasm_val_vec_t res_vec = WASM_ARRAY_VEC(res);
  own wasm_trap_t *trap =
      wasm_func_call(fn, arity ? &args_vec : NULL, has_result ? &res_vec : NULL);
  if (trap == NULL) {
    // Success - extract the result if required.
    if (has_result) {
      result.val = res[0].of.i32;
    }
    result.ok = true;
  } else {
    // Failure - display the error message.
    own wasm_message_t msg;
    wasm_trap_message(trap, &msg);
    error("Error calling '%s': %s", name, msg.data);
    wasm_byte_vec_delete(&msg);
    wasm_trap_delete(trap);
  }
  return result;
}

static wasm_trap_t *print_callback(const wasm_val_vec_t *args, wasm_val_vec_t *results) {
  // args: int len, const char* msg
  assert(args->size == 2);
  assert(args->data[0].kind == WASM_I32);
  assert(args->data[1].kind == WASM_I32);

  // With the C implementation, 'msg' string should be null-terminated.
  char *wasm_memory_base = wasm_memory_data(wc.memory);
  assert(*(wasm_memory_base + args->data[0].of.i32) == 0);

  printf("%s", wasm_memory_data(wc.memory) + args->data[1].of.i32);
  return NULL;
}

static bool init_module() {
  FILE *file = fopen(wc.module_name, "r");
  if (file == NULL) {
    return error("Error loading wasm file");
  }
  fseek(file, 0, SEEK_END);
  wasm_byte_vec_t wasm_bytes;
  wasm_bytes.size = ftell(file);
  wasm_bytes.data = malloc(wasm_bytes.size);
  fseek(file, 0, SEEK_SET);
  fread(wasm_bytes.data, 1, wasm_bytes.size, file);
  fclose(file);

  wc.engine = wasm_engine_new();
  wc.store = wasm_store_new(wc.engine);
  wc.module = wasm_module_new(wc.store, &wasm_bytes);
  if (wc.module == NULL) {
    return error("Error compiling module");
  }
  free(wasm_bytes.data);

  // Set up the 'print_callback' import function.
  own wasm_importtype_vec_t expected_imports;
  wasm_module_imports(wc.module, &expected_imports);
  assert(expected_imports.size == 1);
  wasm_importtype_vec_delete(&expected_imports);

  own wasm_functype_t *print_func_type = wasm_functype_new_1_0(wasm_valtype_new_i32());
  own wasm_func_t *print_func = wasm_func_new(wc.store, print_func_type, print_callback);
  wasm_functype_delete(print_func_type);
  wasm_extern_t *imports[] = { wasm_func_as_extern(print_func) };
  wasm_extern_vec_t import_object = WASM_ARRAY_VEC(imports);

  // Instantiate module.
  wc.instance = wasm_instance_new(wc.store, wc.module, &import_object, NULL);
  wasm_func_delete(print_func);
  if (wc.instance == NULL) {
    return error("Error instantiating module");
  }

  // Retrieve module exports.
  wasm_module_exports(wc.module, &wc.module_exports);
  wasm_instance_exports(wc.instance, &wc.instance_exports);
  assert(wc.module_exports.size == wc.instance_exports.size);

  for (int i = 0; i < wc.module_exports.size; i++) {
    wasm_exporttype_t *export_type = wc.module_exports.data[i];
    const wasm_name_t *name = wasm_exporttype_name(export_type);
    const wasm_externkind_t kind = wasm_externtype_kind(wasm_exporttype_type(export_type));
    wasm_extern_t *instance_extern = wc.instance_exports.data[i];
    if (strncmp("memory", name->data, name->size) == 0) {
      assert(wasm_extern_kind(instance_extern) == WASM_EXTERN_MEMORY);
      wc.memory = wasm_extern_as_memory(instance_extern);
    } else {
      for (int j = 0; j < N_FUNCS; j++) {
        if (strncmp(kExportFuncNames[j], name->data, name->size) == 0) {
          assert(wasm_extern_kind(instance_extern) == WASM_EXTERN_FUNC);
          wc.funcs[j] = wasm_extern_as_func(instance_extern);
          break;
        }
      }
    }
  }
  if (wc.memory == NULL) {
    return error("'memory' export not found");
  }
  for (int i = 0; i < N_FUNCS; i++) {
    if (wc.funcs[i] == NULL) {
      return error("Function export '%s' not found", kExportFuncNames[i]);
    }
  }
  return true;
}

static bool map_shared_bufs() {
  // Call wasm.malloc to reserve enough space for the shared buffers plus alignment concerns.
  int page_size = sysconf(_SC_PAGESIZE);
  int wasm_alloc_size = wc.ro_size + wc.rw_size + 3 * page_size;
  FuncResult wasm_alloc_res = fn_call(FN_MALLOC, wasm_alloc_size);
  if (!wasm_alloc_res.ok) {
    return false;
  }

  // Get the location of wasm's linear memory buffer in our address space.
  void *wasm_memory_base = wasm_memory_data(wc.memory);

  // Convert the reserve alloc's linear address to our address space.
  void *wasm_alloc_ptr = wasm_memory_base + wasm_alloc_res.val;

  // Align the shared buffers inside wasm's linear memory against our page boundaries.
  void *aligned_ro_ptr = page_align(wasm_alloc_ptr, page_size);
  void *aligned_rw_ptr = page_align(aligned_ro_ptr + wc.ro_size, page_size);

  // Verify that our overall mmapped size will be safely contained in the wasm allocation.
  void *end = page_align(aligned_rw_ptr + wc.rw_size, page_size);
  assert(end - wasm_alloc_ptr <= wasm_alloc_size);

  // Map read-only buffer.
  int flags = MAP_SHARED | MAP_FIXED;
  int ro_fd = shm_open(wc.ro_name, O_RDONLY, S_IRUSR | S_IWUSR);
  if (ro_fd == -1) {
    return error("Error calling shm_open");
  }
  wc.ro_buf = mmap(aligned_ro_ptr, wc.ro_size, PROT_READ, flags, ro_fd, 0);
  assert(wc.ro_buf == aligned_ro_ptr);

  // Map read-write buffer.
  int rw_fd = shm_open(wc.rw_name, O_RDWR, S_IRUSR | S_IWUSR);
  if (rw_fd == -1) {
    return error("Error calling shm_open");
  }
  wc.rw_buf = mmap(aligned_rw_ptr, wc.rw_size, PROT_READ | PROT_WRITE, flags, rw_fd, 0);
  assert(wc.rw_buf == aligned_rw_ptr);

  // We don't need the file descriptors once the buffers have been mapped.
  assert(close(rw_fd) != -1 && close(ro_fd) != -1);
  info("  read-only  buffer: %p", wc.ro_buf);
  info("  read-write buffer: %p", wc.rw_buf);

  // Inform the wasm module of the aligned shared buffer location in linear memory.
  int ro_index = (void *)wc.ro_buf - wasm_memory_base;
  int rw_index = (void *)wc.rw_buf - wasm_memory_base;
  return fn_call(FN_SET_SHARED, ro_index, wc.ro_size, rw_index, wc.rw_size).ok;
}

static void destroy() {
  if (wc.rw_buf != NULL) {
    assert(munmap(wc.rw_buf, wc.rw_size) != -1);
  }
  if (wc.ro_buf != NULL) {
    assert(munmap(wc.ro_buf, wc.ro_size) != -1);
  }
  if (wc.instance_exports.data != NULL) {
    wasm_extern_vec_delete(&wc.instance_exports);
  }
  if (wc.module_exports.data != NULL) {
    wasm_exporttype_vec_delete(&wc.module_exports);
  }
  if (wc.instance != NULL) {
    wasm_instance_delete(wc.instance);
  }
  if (wc.module != NULL) {
    wasm_module_delete(wc.module);
  }
  if (wc.store != NULL) {
    wasm_store_delete(wc.store);
  }
  if (wc.engine != NULL) {
    wasm_engine_delete(wc.engine);
  }
}

static void send(Command code) {
  assert(write(wc.write_fd, &code, 1) == 1);
}

static void command_loop() {
  bool ok = true;
  while (ok) {
    char cmd = '-';
    assert(read(wc.read_fd, &cmd, 1) == 1);
    switch (cmd) {
      case CMD_INIT:
        ok = fn_call(FN_INIT, time(NULL)).ok;
        break;
      case CMD_TICK:
        ok = fn_call(FN_TICK).ok;
        break;
      case CMD_EXIT:
        send(cmd);
        return;
      case CMD_MODIFY_GRID:
        ok = fn_call(FN_MODIFY_GRID).ok;
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
  assert(argc == 8);
  wc.module_name = argv[1];
  wc.label = argv[1][strlen(argv[1]) - 11]; // LOL
  wc.read_fd = atoi(argv[2]);
  wc.write_fd = atoi(argv[3]);
  wc.ro_name = argv[4];
  wc.ro_size = atoi(argv[5]);
  wc.rw_name = argv[6];
  wc.rw_size = atoi(argv[7]);

  info("Container started; module '%s', pid %d", wc.module_name, getpid());
  if (init_module() && map_shared_bufs()) {
    send(CMD_READY);
    command_loop();
  }
  destroy();
}
