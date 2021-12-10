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
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include "wasm_c_api.h"

const int INSTANCE_LIMIT = 100;
const int START_DELAY_SECS = 2;
const int LOOP_DELAY_SECS = 1;

wasm_func_t *instantiate(wasm_store_t *store, wasm_module_t *module) {
  wasm_instance_t *instance = wasm_instance_new(store, module, NULL, NULL);
  assert(instance != NULL);

  wasm_exporttype_vec_t module_exports;
  wasm_extern_vec_t instance_exports;
  wasm_module_exports(module, &module_exports);
  wasm_instance_exports(instance, &instance_exports);
  for (int i = 0; i < module_exports.size; i++) {
    wasm_exporttype_t *export_type = module_exports.data[i];
    const wasm_name_t *name = wasm_exporttype_name(export_type);
    if (strncmp("tick", name->data, name->size) == 0) {
      return wasm_extern_as_func(instance_exports.data[i]);
    }
  }
  assert(false);
}

int main(int argc, const char *argv[]) {
  printf("container-wamr %s\n", argv[1]);

  FILE *file = fopen(argv[1], "r");
  assert(file != NULL);
  fseek(file, 0, SEEK_END);
  wasm_byte_vec_t wasm_bytes;
  wasm_bytes.size = ftell(file);
  wasm_bytes.data = malloc(wasm_bytes.size);
  fseek(file, 0, SEEK_SET);
  fread(wasm_bytes.data, 1, wasm_bytes.size, file);
  fclose(file);

  wasm_engine_t *engine = wasm_engine_new();
  wasm_store_t *store = wasm_store_new(engine);
  wasm_module_t *module = wasm_module_new(store, &wasm_bytes);
  free(wasm_bytes.data);
  assert(module != NULL);

  wasm_func_t *tick_fn[INSTANCE_LIMIT];
  int counter[INSTANCE_LIMIT];
  memset(tick_fn, 0, sizeof(tick_fn));
  memset(counter, 0, sizeof(counter));
  sleep(START_DELAY_SECS);

  int wi = 0;
  while (true) {
    if (wi < INSTANCE_LIMIT) {
      tick_fn[wi++] = instantiate(store, module);
    }
    for (int i = 0; i < wi; i++) {
      wasm_val_t res[1] = { WASM_INIT_VAL };
      wasm_val_vec_t res_vec = WASM_ARRAY_VEC(res);
      wasm_func_call(tick_fn[i], NULL, &res_vec);
      assert(res[0].of.i32 == ++counter[i]);
    }
    sleep(LOOP_DELAY_SECS);
  }
}
