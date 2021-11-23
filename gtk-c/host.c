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
#include <string.h>
#include <time.h>
#include <unistd.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <gtk/gtk.h>
#include "common.h"

const char *kReadOnlyBufName = "/shared_ro";
const char *kReadWriteBufName = "/shared_rw";
const int kReadOnlyBufSize = sizeof(int[GRID_H][GRID_W]);
const int kReadWriteBufSize = sizeof(Hunter) + N_RUNNERS * sizeof(Runner);

#define R 0
#define W 1

typedef struct {
  int p2c[2];
  int c2p[2];
} Pipes;

typedef struct {
  Pipes pipes[2];
  void *shared_ro;
  void *shared_rw;
  bool enable_host_modify;
} Context;

Context ctx = { 0 };

static void *create_shared_buffer(const char *name, int size) {
  // shm_open() creates the actual memory buffer for sharing.
  int fd = shm_open(name, O_CREAT | O_TRUNC | O_RDWR, S_IRUSR | S_IWUSR);
  assert(fd != -1);
  assert(ftruncate(fd, size) != -1);

  // mmap() allows the host to access the shared buffers (initialise, read for GUI display, etc).
  char *shared = mmap(NULL, size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
  assert(close(fd) != -1);
  return shared;
}

static void fork_container(Pipes *pipes, const char *module) {
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
    execlp("./container", "container", module, read_fd, write_fd,
           kReadOnlyBufName, ro_size, kReadWriteBufName, rw_size, NULL);
    assert(false);  // should not be reached
  } else {
    // Parent
    close(pipes->p2c[R]);
    close(pipes->c2p[W]);

    // Wait for the ready signal from the container binary.
    char ready;
    assert(read(pipes->c2p[R], &ready, 1) == 1);
    assert(ready == CMD_READY);
  }
}

static bool send(Command code) {
  for (int i = 0; i < 2; i++) {
    assert(write(ctx.pipes[i].p2c[W], &code, 1) == 1);

    char ack = '-';
    assert(read(ctx.pipes[i].c2p[R], &ack, 1) == 1);
    if (ack == CMD_FAILED) {
      printf(">> Received failure signal, aborting\n");
      return false;
    }
    if (ack != code) {
      printf(">> Incorrect ack '%c' received for command '%c'\n", ack, code);
      return false;
    }
  }
  return true;
}

static void init_grid() {
  int (*grid)[GRID_W] = ctx.shared_ro;
  memset(grid, 0, kReadOnlyBufSize);
  for (int x = 0; x < GRID_W; x++) {
    grid[0][x] = grid[GRID_H - 1][x] = 1;
  }
  for (int y = 1; y < GRID_H - 1; y++) {
    grid[y][0] = grid[y][GRID_W - 1] = 1;
  }
  for (int i = 0; i < N_BLOCKS; i++) {
    int x = 1 + rand() % (GRID_W - 2);
    int y = 1 + rand() % (GRID_H - 2);
    grid[y][x] = 1;
  }
}

#if 0
static void print_grid() {
  int grid[GRID_H][GRID_W];
  int *gp = &grid[0][0];
  int *sp = ctx.shared_ro;
  for (int i = 0; i < GRID_H * GRID_W; i++) {
    *gp++ = *sp++ ? '#' : ' ';
  }

  Runner *r = ctx.shared_rw + sizeof(Hunter);
  for (int i = 0; i < N_RUNNERS; r++, i++) {
    grid[r->y][r->x] = "~+%"[r->state];
  }

  Hunter *h = ctx.shared_rw;
  grid[h->y][h->x] = 'X';

  for (int y = 0; y < GRID_H; y++) {
    for (int x = 0; x < GRID_W; x++) {
      printf("%c ", grid[y][x]);
    }
    printf("\n");
  }
}
#endif

static gboolean tick(gpointer data) {
  if (ctx.enable_host_modify) {
    int (*grid)[GRID_W] = ctx.shared_ro;
    for (int i = 0; i < 5; i++) {
      int x = 1 + rand() % (GRID_W - 2);
      int y = 1 + rand() % (GRID_H - 2);
      grid[y][x] = 1 - grid[y][x];
    }
  }
  assert(send(CMD_TICK));
  gtk_widget_queue_draw(data);
  return true;
}

static gboolean delay_start(gpointer data) {
  g_timeout_add(TICK_MS, tick, data);
  return false;
}

static void draw_fn(GtkDrawingArea *area, cairo_t *cr, int width, int height, gpointer data) {
  // Super dumb animation: just redraw the whole field, every time.
  cairo_set_source_rgb(cr, 1, 1, 0.95);
  cairo_rectangle(cr, 0, 0, width, height);
  cairo_fill(cr);

  int (*grid)[GRID_W] = ctx.shared_ro;
  for (int y = 0; y < GRID_H; y++) {
    for (int x = 0; x < GRID_W; x++) {
      if (grid[y][x] == 1) {
        cairo_set_source_rgb(cr, 0.3, 0.3, 0.3);
        cairo_rectangle(cr, x * SCALE, y * SCALE, SCALE, SCALE);
        cairo_fill(cr);
      }
    }
  }

  Hunter *h = ctx.shared_rw;
  cairo_set_source_rgb(cr, 0.8, 0.5, 0.9);
  cairo_rectangle(cr, h->x * SCALE, h->y * SCALE, SCALE, SCALE);
  cairo_fill(cr);

  Runner *r = ctx.shared_rw + sizeof(Hunter);
  for (int i = 0; i < N_RUNNERS; r++, i++) {
    switch (r->state) {
      case WALKING:
        cairo_set_source_rgb(cr, 0.5, 0.8, 0.9);
        break;
      case RUNNING:
        cairo_set_source_rgb(cr, 1, 0.8, 0.5);
        break;
      case DEAD:
        cairo_set_source_rgb(cr, 1, 0.4, 0.4);
        break;
    }
    cairo_arc(cr, r->x * SCALE + SCALE / 2, r->y * SCALE + SCALE / 2, SCALE / 2, 0, 2 * G_PI);
    cairo_fill(cr);
  }
}

static void host_modify(GtkWidget *button, gpointer data) {
  ctx.enable_host_modify = !ctx.enable_host_modify;
}

static void container_modify(GtkWidget *button, gpointer data) {
  // Crashes!
  send(CMD_MODIFY_GRID);
}

static void app_close(GtkWidget *button, gpointer data) {
  g_application_quit(G_APPLICATION(data));
}

static void on_activate(GtkApplication *app, gpointer data) {
  GtkWidget *window = gtk_application_window_new(app);
  gtk_window_set_title(GTK_WINDOW(window), "WebAssembly shared buffers [C]");

  GtkWidget *drawing_area = gtk_drawing_area_new();
  gtk_drawing_area_set_content_width(GTK_DRAWING_AREA(drawing_area), GRID_W * SCALE);
  gtk_drawing_area_set_content_height(GTK_DRAWING_AREA(drawing_area), GRID_H * SCALE);
  gtk_drawing_area_set_draw_func(GTK_DRAWING_AREA(drawing_area), draw_fn, NULL, NULL);

  GtkWidget *host_modify_btn = gtk_button_new_with_label("Host modifies grid");
  g_signal_connect(host_modify_btn, "clicked", G_CALLBACK(host_modify), NULL);

  GtkWidget *container_modify_btn = gtk_button_new_with_label("Container modifies grid");
  g_signal_connect(container_modify_btn, "clicked", G_CALLBACK(container_modify), NULL);

  GtkWidget *close_btn = gtk_button_new_with_label("Close");
  g_signal_connect(close_btn, "clicked", G_CALLBACK(app_close), app);

  GtkWidget *hbox = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 10);
  gtk_box_append(GTK_BOX(hbox), host_modify_btn);
  gtk_box_append(GTK_BOX(hbox), container_modify_btn);
  gtk_box_append(GTK_BOX(hbox), close_btn);

  GtkWidget *vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 10);
  gtk_box_append(GTK_BOX(vbox), drawing_area);
  gtk_box_append(GTK_BOX(vbox), hbox);

  gtk_window_set_child(GTK_WINDOW(window), vbox);
  gtk_window_present(GTK_WINDOW(window));
  g_timeout_add(500, delay_start, drawing_area);
}

static void on_shutdown(GtkApplication *app, gpointer data) {
  assert(send(CMD_EXIT));
  wait(NULL);

  assert(munmap(ctx.shared_ro, kReadOnlyBufSize) != -1);
  assert(munmap(ctx.shared_rw, kReadWriteBufSize) != -1);
  assert(shm_unlink(kReadOnlyBufName) != -1);
  assert(shm_unlink(kReadWriteBufName) != -1);
}

int main(int argc, char *argv[]) {
  printf("Host started; pid %d\n", getpid());
  ctx.shared_ro = create_shared_buffer(kReadOnlyBufName, kReadOnlyBufSize);
  ctx.shared_rw = create_shared_buffer(kReadWriteBufName, kReadWriteBufSize);

  srand(time(NULL));
  init_grid();
  if (argc <= 2) {
    printf("usage: host hunter.wasm runner.wasm");
  }
  fork_container(&ctx.pipes[0], argv[1]); // Path to hunter.wasm
  fork_container(&ctx.pipes[1], argv[2]); // Path to runner.wasm
  assert(send(CMD_INIT));

  GtkApplication *app = gtk_application_new(NULL, G_APPLICATION_HANDLES_OPEN);
  g_signal_connect(app, "activate", G_CALLBACK(on_activate), &ctx);
  g_signal_connect(app, "open", G_CALLBACK(on_activate), &ctx);
  g_signal_connect(app, "shutdown", G_CALLBACK(on_shutdown), &ctx);
  return g_application_run(G_APPLICATION(app), argc, argv);
}
