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
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/mman.h>
#include "wasm_c_api.h"

enum ExportFuncs {
  FN_MALLOC,
  FN_TEST_OVERFLOW_ATTACK,
};

const char *kExportFuncNames[] = {
  "malloc",
  "test_overflow_attack",
};

#include "../wamr-wrapper.c"

// Reserves the first 2 * page_size in the heap then mprotect's the aligned page area
// at the end of that region.
static void apply_heap_guard() {
  size_t page_size = sysconf(_SC_PAGESIZE);
  int guard_size = 2 * page_size;
  int guard_alloc = wasm_call(FN_MALLOC, guard_size).val;
  void *guard_alloc_end = wasm_memory_data(wc.memory) + guard_alloc + guard_size;
  void *end_page = (void *)(((size_t)guard_alloc_end - page_size) & ~(page_size - 1));
  int res = mprotect(end_page, page_size, PROT_NONE);
  assert(res == 0);
}


int main(int argc, const char *argv[]) {
  if (init_module("module.wasm")) {
    if (argc > 1 && *argv[1] == '+') {
      apply_heap_guard();
    }
    wasm_call(FN_TEST_OVERFLOW_ATTACK);
  }
  destroy_module();
}
