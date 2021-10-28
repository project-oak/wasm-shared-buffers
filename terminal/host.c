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
#include <assert.h>
#include <fcntl.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/mman.h>
#include <sys/wait.h>

const char *kReadOnlyBufName = "/shared_ro";
const char *kReadWriteBufName = "/shared_rw";
const int kReadOnlyBufSize = 5000;
const int kReadWriteBufSize = 1000;

#define R 0
#define W 1

typedef struct {
  int p2c[2];
  int c2p[2];
} Pipes;

void *setup_shared_buf(const char *name, int size, const char *mode) {
  int fd = shm_open(name, O_CREAT | O_RDWR, S_IRUSR | S_IWUSR);
  if (fd == -1) {
    perror("shm_open");
    exit(1);
  }
  if (ftruncate(fd, size) == -1) {
    perror("ftruncate");
    exit(1);
  }

  // Fill the shared buffer for verification in wasm.
  char *shared = mmap(NULL, size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
  sprintf(shared, "%s:", mode);
  sprintf(shared + size - 3, "buf");
  char v[2] = { 131, 173 };
  for (int i = 3; i < size - 3; i++) {
    shared[i] = v[i % 2];
  }
  assert(close(fd) != -1);
  return shared;
}

void fork_container(Pipes *pipes, const char *label) {
  assert(pipe(pipes->p2c) == 0);
  assert(pipe(pipes->c2p) == 0);

  if (fork() == 0) {
    // Child
    close(pipes->p2c[W]);
    close(pipes->c2p[R]);

    char read_fd[3];
    char write_fd[3];
    char ro_size[20];
    char rw_size[20];
    sprintf(read_fd, "%d", pipes->p2c[R]);
    sprintf(write_fd, "%d", pipes->c2p[W]);
    sprintf(ro_size, "%d", kReadOnlyBufSize);
    sprintf(rw_size, "%d", kReadWriteBufSize);
    execlp("./container", "container", label, read_fd, write_fd,
           kReadOnlyBufName, ro_size, kReadWriteBufName, rw_size, NULL);
    perror("exec");
    assert(false);  // should not be reached
  } else {
    // Parent
    close(pipes->p2c[R]);
    close(pipes->c2p[W]);

    // Wait for the ready signal from the container binary.
    char ready;
    assert(read(pipes->c2p[R], &ready, 1) == 1);
    assert(ready == '@');
  }
}

bool send(Pipes *pipes, char cmd) {
  assert(write(pipes->p2c[W], &cmd, 1) == 1);

  char ack = '-';
  assert(read(pipes->c2p[R], &ack, 1) == 1);
  if (ack == '*') {
    printf(">> Received failure signal, aborting\n");
    return false;
  }
  if (ack != cmd) {
    printf(">> Incorrect ack '%c' received for command '%c'\n", ack, cmd);
    return false;
  }
  return true;
}

int main(int argc, const char *argv[]) {
  printf("Creating shared memory buffers\n");
  void *shared_ro = setup_shared_buf(kReadOnlyBufName, kReadOnlyBufSize, "ro");
  void *shared_rw = setup_shared_buf(kReadWriteBufName, kReadWriteBufSize, "rw");

  Pipes pipes[2];
  fork_container(&pipes[0], "A");
  fork_container(&pipes[1], "B");

  // Writes to read-only buffer (crashes).
  //const char *cmds[] = { "bx", "ai", "aq", "ax" };

  // Sequential write-read.
  //const char *cmds[] = { "ai", "aw", "ax", "bi", "br", "bx" };

  // Concurrent write-read with memory tests.
  const char *cmds[] = { "ai", "av", "bi", "bv", "am", "aw", "br", "bm", "ax", "bx" };

  for (int i = 0; i < sizeof(cmds) / sizeof(*cmds); i++) {
    if (!send(&pipes[cmds[i][0] - 'a'], cmds[i][1]))
      break;
  }
  wait(NULL);

  printf("\nDeleting shared memory buffers\n");
  if (munmap(shared_ro, kReadOnlyBufSize) == -1 || munmap(shared_rw, kReadWriteBufSize) == -1) {
    perror("munmap");
    return 1;
  }
  if (shm_unlink(kReadOnlyBufName) == -1 || shm_unlink(kReadWriteBufName) == -1) {
    perror("shm_unlink");
    return 1;
  }
  return 0;
}
