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
#include <unistd.h>
#include <sys/mman.h>
#include "wasm_c_api.h"

// Ownership indicator as used by the wasm-c-api code.
#define own

enum ExportFuncs {
  FN_MALLOC,
  FN_SET_SHARED,
  FN_VERIFY_SHARED,
  FN_FILL_MEMORY,
  FN_CLEAR_MEMORY,
  FN_WRITE_RW,
  FN_READ_RW,
  FN_WRITE_RO,
  FN_FORCE_ERROR,
};

const char *kExportFuncNames[] = {
  "malloc",
  "set_shared",
  "verify_shared",
  "fill_memory",
  "clear_memory",
  "write_rw",
  "read_rw",
  "write_ro",
  "force_error",
};

#define N_FUNCS  (sizeof(kExportFuncNames) / sizeof(*kExportFuncNames))

typedef struct {
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
void *page_align(void *p, size_t page_size) {
  return (void *)((((size_t)p - 1) & ~(page_size - 1)) + page_size);
}

void info(const char *fmt, ...) {
  char msg[500];
  va_list ap;
  va_start(ap, fmt);
  vsnprintf(msg, 500, fmt, ap);
  va_end(ap);
  printf("[%c] %s\n", wc.label, msg);
}

bool error(const char *fmt, ...) {
  char msg[500];
  va_list ap;
  va_start(ap, fmt);
  vsnprintf(msg, 500, fmt, ap);
  va_end(ap);
  fprintf(stderr, "[%c] >> %s\n", wc.label, msg);
  return false;
}

const char *kind_str(enum wasm_externkind_enum kind) {
  switch (kind) {
    case WASM_EXTERN_FUNC:   return "WASM_EXTERN_FUNC";
    case WASM_EXTERN_GLOBAL: return "WASM_EXTERN_GLOBAL";
    case WASM_EXTERN_TABLE:  return "WASM_EXTERN_TABLE";
    case WASM_EXTERN_MEMORY: return "WASM_EXTERN_MEMORY";
    default:                 return "(unknown kind)";
  }
}

typedef struct {
  bool ok;
  int val;
} FuncResult;

// Wraps the cumbersome wasm_func_call API. Assumes args and return value, where present, are i32.
FuncResult fn_call(int index, ...) {
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
  char buf[500];
  char *p = buf;
  p += sprintf(p, "  -- calling %s(", name);
  va_list ap;
  va_start(ap, index);
  for (int i = 0; i < arity; i++) {
    int val = va_arg(ap, int);
    args[i].kind = WASM_I32;
    args[i].of.i32 = val;
    args_vec.num_elems++;
    p += sprintf(p, "%s%d", i ? ", " : "", val);
  }
  va_end(ap);
  info("%s)", buf);

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

bool init_module() {
  info("Loading wasm file");
  FILE *file = fopen("module.wasm", "r");
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

  info("Creating the store");
  wc.engine = wasm_engine_new();
  wc.store = wasm_store_new(wc.engine);

  info("Compiling module");
  wc.module = wasm_module_new(wc.store, &wasm_bytes);
  if (wc.module == NULL) {
    return error("Error compiling module");
  }
  free(wasm_bytes.data);

  info("Checking module imports");
  own wasm_importtype_vec_t expected_imports;
  wasm_module_imports(wc.module, &expected_imports);
  if (expected_imports.size > 0) {
    return error("Module expects %d imports", expected_imports.size);
  }
  wasm_importtype_vec_delete(&expected_imports);

  info("Instantiating module");
  wc.instance = wasm_instance_new(wc.store, wc.module, NULL, NULL);
  if (wc.instance == NULL) {
    return error("Error instantiating module");
  }

  info("Retrieving module exports");
  wasm_module_exports(wc.module, &wc.module_exports);
  wasm_instance_exports(wc.instance, &wc.instance_exports);
  assert(wc.module_exports.size == wc.instance_exports.size);

  for (int i = 0; i < wc.module_exports.size; i++) {
    wasm_exporttype_t *export_type = wc.module_exports.data[i];
    const wasm_name_t *name = wasm_exporttype_name(export_type);
    const wasm_externkind_t kind = wasm_externtype_kind(wasm_exporttype_type(export_type));

    char buf[100] = { 0 };
    memcpy(buf, name->data, name->size);
    info("  %-30s %s", buf, kind_str(kind));

    wasm_extern_t *instance_extern = wc.instance_exports.data[i];
    if (strcmp("memory", buf) == 0) {
      assert(wasm_extern_kind(instance_extern) == WASM_EXTERN_MEMORY);
      wc.memory = wasm_extern_as_memory(instance_extern);
    } else {
      for (int j = 0; j < N_FUNCS; j++) {
        if (strcmp(kExportFuncNames[j], buf) == 0) {
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

bool init_shared_bufs() {
  info("Allocating shared buffer space in wasm");
  int page_size = sysconf(_SC_PAGESIZE);

  // Call wasm.malloc to reserve enough space for the shared buffers plus alignment concerns.
  int wasm_alloc_size = wc.ro_size + wc.rw_size + 3 * page_size;
  info("  wasm_alloc_size: %d", wasm_alloc_size);
  FuncResult malloc_res = fn_call(FN_MALLOC, wasm_alloc_size);
  if (!malloc_res.ok) {
    return false;
  }

  // Get the location of wasm's linear memory buffer in our address space.
  void *wasm_memory_base = wasm_memory_data(wc.memory);
  info("  wasm_memory_base: %p", wasm_memory_base);

  // Convert the reserve alloc's linear address to our address space.
  void *wasm_alloc_ptr = wasm_memory_base + malloc_res.val;
  info("  wasm_alloc_index: %d", malloc_res.val);
  info("  wasm_alloc_ptr:   %p", wasm_alloc_ptr);

  // Align the shared buffers inside wasm's linear memory against our page boundaries.
  void *aligned_ro = page_align(wasm_alloc_ptr, page_size);
  void *aligned_rw = page_align(aligned_ro + wc.ro_size, page_size);
  info("  aligned_ro:       %p", aligned_ro);
  info("  aligned_rw:       %p", aligned_rw);

  // Verify that our overall mmapped size will be safely contained in the wasm allocation.
  void *end = page_align(aligned_rw + wc.rw_size, page_size);
  int aligned_size = end - wasm_alloc_ptr;
  info("  aligned_size:     %d", aligned_size);
  assert(aligned_size <= wasm_alloc_size);

  info("Mapping read-only buffer");
  int flags = MAP_SHARED | MAP_FIXED;
  int ro_fd = shm_open(wc.ro_name, O_RDONLY, S_IRUSR | S_IWUSR);
  if (ro_fd == -1) {
    return error("Error calling shm_open");
  }
  wc.ro_buf = mmap(aligned_ro, wc.ro_size, PROT_READ, flags, ro_fd, 0);
  assert(wc.ro_buf == aligned_ro);

  info("Mapping read-write buffer");
  int rw_fd = shm_open(wc.rw_name, O_RDWR, S_IRUSR | S_IWUSR);
  if (rw_fd == -1) {
    return error("Error calling shm_open");
  }
  wc.rw_buf = mmap(aligned_rw, wc.rw_size, PROT_READ | PROT_WRITE, flags, rw_fd, 0);
  assert(wc.rw_buf == aligned_rw);

  // We don't need the file descriptors once the buffers have been mapped.
  assert(close(rw_fd) != -1 && close(ro_fd) != -1);

  // Inform the wasm module of the aligned shared buffer location in linear memory.
  int shift_ro = (void *)wc.ro_buf - wasm_memory_base;
  int shift_rw = (void *)wc.rw_buf - wasm_memory_base;
  info("  shift_ro: %d", shift_ro);
  info("  shift_rw: %d", shift_rw);
  return fn_call(FN_SET_SHARED, shift_ro, wc.ro_size, shift_rw, wc.rw_size).ok;
}

void destroy() {
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

bool verify_shared_bufs() {
  info("Verifying shared buffers");
  FuncResult res = fn_call(FN_VERIFY_SHARED);
  assert(res.ok);
  switch (res.val) {
    case 0:
      return true;
    case 1:
      return error("failed: prefix token not matched");
    case 2:
      return error("failed: suffix token not matched");
    default:
      return error("failed: incorrect value at index %d", res.val);
  }
}

void scan_memory() {
  unsigned char *p = wasm_memory_data(wc.memory);
  size_t size = wasm_memory_data_size(wc.memory);
  size_t non_zero = 0;
  size_t filled = 0;
  for (size_t i = 0; i < size; i++, p++) {
    if (*p != 0)
      non_zero++;
    if (*p == 181)
      filled++;
  }
  info("  %.1lf%% non-zero, %.1lf%% filled", (100.0 * non_zero) / size, (100.0 * filled) / size);
}

bool test_memory_alloc() {
  info("Performing memory allocation test");
  scan_memory();
  FuncResult res = fn_call(FN_FILL_MEMORY);
  assert(res.ok);
  info("  malloc failed on iteration %d", res.val);
  scan_memory();
  assert(fn_call(FN_CLEAR_MEMORY).ok);
  scan_memory();
  return true;
}

bool write_to_rw() {
  info("Writing to read-write buffer");

  // Write 10 values of 20,21,22... from index 3.
  assert(fn_call(FN_WRITE_RW, 3, 20, 10).ok);
  return true;
}

bool read_from_rw() {
  info("Reading from read-write buffer");
  FuncResult res = fn_call(FN_READ_RW, 3, 20, 10);
  assert(res.ok);
  if (res.val != 0)
    return error("failed");
  return true;
}

void write_to_ro() {
  info("Attempting a write to read-only buffer");
  fn_call(FN_WRITE_RO);
  info("-- should not be reached --");
}

bool test_error_handling() {
  info("Testing container error handling in wasm function call");
  return !fn_call(FN_FORCE_ERROR).ok;
}

void send(char code) {
  assert(write(wc.write_fd, &code, 1) == 1);
}

// Commands:
//   i: initialise
//   v: verify shared memory contents
//   m: test memory allocation
//   w: write to read-write buffer
//   r: read from read-write buffer
//   q: write to read-only buffer (will crash)
//   e: test container's handling of errors in wasm function calls
//   x: exit
void command_loop() {
  bool ok = true;
  while (ok) {
    char cmd = '-';
    assert(read(wc.read_fd, &cmd, 1) == 1);
    printf("\n");
    info("<cmd> %c", cmd);
    switch (cmd) {
      case 'i':
        ok = init_module() && init_shared_bufs();
        break;
      case 'v':
        ok = verify_shared_bufs();
        break;
      case 'm':
        ok = test_memory_alloc();
        break;
      case 'w':
        ok = write_to_rw();
        break;
      case 'r':
        ok = read_from_rw();
        break;
      case 'q':
        write_to_ro();  // crashes!
        break;
      case 'e':
        ok = test_error_handling();
        break;
      case 'x':
        send(cmd);
        return;
      default:
        info("  ?? unknown command code");
        break;
    }
    if (ok) {
      info("  success");
      // Send ack to host.
      send(cmd);
    } else {
      // Send failure signal to host.
      send('*');
    }
  }
}

int main(int argc, const char *argv[]) {
  assert(argc == 8);
  wc.label = *argv[1];
  wc.read_fd = atoi(argv[2]);
  wc.write_fd = atoi(argv[3]);
  wc.ro_name = argv[4];
  wc.ro_size = atoi(argv[5]);
  wc.rw_name = argv[6];
  wc.rw_size = atoi(argv[7]);
  info("Container started; pid %d", getpid());

  // Send ready signal to host.
  send('@');

  // Process commands from host.
  command_loop();

  info("Shutting down");
  destroy();
}
