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

// Inlined via #include in gtk/container.c and heap-guard/container.c

#define N_FUNCS  (sizeof(kExportFuncNames) / sizeof(*kExportFuncNames))

// Ownership indicator as used by the wasm-c-api code.
#define own

typedef struct {
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
} WasmComponents;

WasmComponents wc = { 0 };

typedef struct {
  bool ok;
  int val;
} CallResult;

// Wraps the cumbersome wasm_func_call API. Assumes args and return value, where present, are i32.
static CallResult wasm_call(int index, ...) {
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
  CallResult result = { false, 0 };
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
    fprintf(stderr, "Error calling '%s': %s", name, msg.data);
    wasm_byte_vec_delete(&msg);
    wasm_trap_delete(trap);
  }
  return result;
}

static wasm_trap_t *print_callback(const wasm_val_vec_t *args, wasm_val_vec_t *results) {
  // args: int len, const char *msg
  assert(args->size == 2);
  assert(args->data[0].kind == WASM_I32);
  assert(args->data[1].kind == WASM_I32);

  // With the C implementation, 'msg' string should be null-terminated.
  char *wasm_memory_base = wasm_memory_data(wc.memory);
  assert(*(wasm_memory_base + args->data[0].of.i32) == 0);

  printf("%s", wasm_memory_data(wc.memory) + args->data[1].of.i32);
  return NULL;
}

static bool init_module(const char *module_name) {
  FILE *file = fopen(module_name, "r");
  if (file == NULL) {
    fprintf(stderr, "Error loading wasm file '%s'", module_name);
    return false;
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
    fprintf(stderr, "Error compiling module");
    return false;
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
    fprintf(stderr, "Error instantiating module");
    return false;
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
    fprintf(stderr, "'memory' export not found");
    return false;
  }
  for (int i = 0; i < N_FUNCS; i++) {
    if (wc.funcs[i] == NULL) {
      fprintf(stderr, "Function export '%s' not found", kExportFuncNames[i]);
      return false;
    }
  }
  return true;
}

static void destroy_module() {
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
