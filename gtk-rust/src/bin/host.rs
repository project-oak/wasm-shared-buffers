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
use common::*;
use fork::{fork, Fork};
use gtk::{cairo, gio, prelude::*};
use libc::{MAP_SHARED, O_CREAT, O_RDWR, O_TRUNC, PROT_READ, PROT_WRITE, S_IRUSR, S_IWUSR};
use rand::Rng;
use std::{cell::RefCell, ffi::CString, process, rc::Rc, slice, thread, time::Duration};

fn main() {
    println!("Host started; pid {}", process::id());
    let ctx = Rc::new(RefCell::new(HostContext::new()));
    let app = gtk::Application::new(None, gio::ApplicationFlags::FLAGS_NONE);
    {
        let ctx = ctx.clone();
        app.connect_activate(move |app| on_activate(ctx.clone(), app));
    }
    app.connect_shutdown(move |_app| {
        // glib's timeout infrastructure holds a references that prevents HostContext
        // from being dropped. We need to clear the timeout to fix this.
        glib::source::source_remove(ctx.borrow_mut().timeout_id.take().unwrap());
    });
    app.run();
    println!("Host stopping");
}

struct HostContext<'a> {
    grid: Grid<'a>,
    actors: Actors<'a>,
    shared_ro: cptr,
    shared_rw: cptr,
    timeout_id: Option<glib::source::SourceId>,
    enable_host_modify: bool,
}

impl HostContext<'_> {
    fn new() -> Self {
        let shared_ro = create_shared_buffer(READ_ONLY_BUF_NAME, READ_ONLY_BUF_SIZE);
        let shared_rw = create_shared_buffer(READ_WRITE_BUF_NAME, READ_WRITE_BUF_SIZE);
        fork_container("hunter.wasm", HUNTER_SIGNAL_INDEX);
        fork_container("runner.wasm", RUNNER_SIGNAL_INDEX);

        // Grid and Actors do *not* take ownership of the shared buffers.
        let mut ctx = Self {
            grid: Grid::new(shared_ro, READ_ONLY_BUF_SIZE),
            actors: Actors::new(shared_rw, READ_WRITE_BUF_SIZE),
            shared_ro,
            shared_rw,
            timeout_id: None,
            enable_host_modify: false,
        };
        ctx.grid.init();
        ctx.actors.send_signal(Signal::Init, true);
        ctx
    }

    fn toggle_host_modify(&mut self) {
        self.enable_host_modify = !self.enable_host_modify;
    }
}

impl Drop for HostContext<'_> {
    fn drop(&mut self) {
        self.actors.send_signal(Signal::Exit, false);

        let cname_ro = CString::new(READ_ONLY_BUF_NAME).unwrap();
        let cname_rw = CString::new(READ_WRITE_BUF_NAME).unwrap();
        unsafe {
            if libc::munmap(self.shared_ro, READ_ONLY_BUF_SIZE as usize) == -1 {
                println!("munmap failed for shared_ro");
            }
            if libc::munmap(self.shared_rw, READ_ONLY_BUF_SIZE as usize) == -1 {
                println!("munmap failed for shared_rw");
            }
            if libc::shm_unlink(cname_ro.as_ptr()) == -1 {
                println!("shm_unlink failed for shared_ro");
            }
            if libc::shm_unlink(cname_rw.as_ptr()) == -1 {
                println!("shm_unlink failed for shared_rw");
            }
        }
    }
}

fn create_shared_buffer(name: &str, size: i32) -> cptr {
    let cname = CString::new(name).unwrap();
    unsafe {
        // shm_open() creates the actual memory buffer for sharing.
        let fd = libc::shm_open(cname.as_ptr(), O_CREAT | O_TRUNC | O_RDWR, S_IRUSR | S_IWUSR);
        if fd == -1 {
            panic!("shm_open failed");
        }
        if libc::ftruncate(fd, size as i64) == -1 {
            panic!("ftruncate failed");
        }

        // mmap() allows the host to access the shared buffers (initialise, read for GUI display, etc).
        let buf = libc::mmap(std::ptr::null_mut(), size as usize, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
        if libc::close(fd) == -1 {
            panic!("close failed");
        }
        buf
    }
}

fn fork_container(module: &str, signal_index: usize) {
    match fork() {
        Ok(Fork::Parent(_)) => (),
        Ok(Fork::Child) => {
            let err = exec::execvp("target/debug/container", &["container", module, &signal_index.to_string()]);
            panic!("exec failed: {}", err); // should not be reached
        }
        Err(_) => panic!("fork failed"),
    }
}

// Wraps the (unowned) read-only buffer to provide 2D-array-style access.
struct Grid<'a> {
    data: &'a mut [i32],
}

impl Grid<'_> {
    fn new(shared_ro: cptr, len: i32) -> Self {
        Self {
            data: unsafe { slice::from_raw_parts_mut(shared_ro as *mut i32, len as usize) },
        }
    }

    fn init(&mut self) {
        for x in 0..GRID_W {
            self.set(x, 0, 1);
            self.set(x, GRID_H - 1, 1);
        }
        for y in 1..(GRID_H - 1) {
            self.set(0, y, 1);
            self.set(GRID_W - 1, y, 1);
            for x in 1..(GRID_W - 1) {
                self.set(x, y, 0);
            }
        }
        for _ in 0..N_BLOCKS {
            let x = rand_range(1, GRID_W - 2);
            let y = rand_range(1, GRID_H - 2);
            self.set(x, y, 1);
        }
    }

    fn modify(&mut self) {
        for _ in 0..5 {
            let x = rand_range(1, GRID_W - 2);
            let y = rand_range(1, GRID_H - 2);
            self.set(x, y, 1 - self.get(x, y));
        }
    }

    fn get(&self, x: i32, y: i32) -> i32 {
        self.data[(y * GRID_W + x) as usize]
    }

    fn set(&mut self, x: i32, y: i32, val: i32) {
        self.data[(y * GRID_W + x) as usize] = val;
    }
}

fn rand_range(a: i32, b: i32) -> i32 {
    rand::thread_rng().gen_range(a..=b)
}

// Wraps the (unowned) read-write buffer to provide access to the hunter and runner
// data and to manage communication between the host and container processes.
struct Actors<'a> {
    // Layout: [sig0, sig1, hx, hy, r0x, r0y, r0s, r1x, r1y, r1s, ...]
    data: &'a mut [i32],
}

impl Actors<'_> {
    fn new(shared_rw: cptr, len: i32) -> Self {
        Self {
            data: unsafe { slice::from_raw_parts_mut(shared_rw as *mut i32, len as usize) },
        }
    }

    // IPC is handled with a simple polling loop. The host always moves from zero (Signal::Idle)
    // to non-zero and the containers always move from non-zero to zero. Each container has a
    // dedicated i32 value in the read-write buffer.
    fn send_signal(&mut self, signal: Signal, wait_for_idle: bool) {
        self.data[HUNTER_SIGNAL_INDEX] = signal as i32;
        self.data[RUNNER_SIGNAL_INDEX] = signal as i32;
        if wait_for_idle {
            let idle = Signal::Idle as i32;
            for _ in 0..SIGNAL_REPS {
                if self.data[HUNTER_SIGNAL_INDEX] == idle && self.data[RUNNER_SIGNAL_INDEX] == idle {
                    return;
                }
                thread::sleep(Duration::from_millis(SIGNAL_WAIT));
            }
            panic!("failed to receive idle signal");
        }
    }

    fn hunter(&self) -> Position {
        // Hunter co-ords are after the 2 * i32 signal values.
        Position { x: self.data[2], y: self.data[3] }
    }

    fn runner(&self, index: i32) -> (Position, State) {
        // Runners start after 2 * i32 signal values + 2 * i32 hunter co-ords.
        let i = 4 + 3 * index as usize;
        (
            Position { x: self.data[i], y: self.data[i + 1] },
            State::from(self.data[i + 2]),
        )
    }
}

struct Position {
    x: i32,
    y: i32,
}

#[derive(Copy, Clone, PartialEq)]
enum State {
    Walking,
    Running,
    Dead,
}

impl State {
    pub fn from(value: i32) -> Self {
        assert!(value >= 0 && value < 3);
        [Self::Walking, Self::Running, Self::Dead][value as usize]
    }
}

fn on_activate(ctx: Rc<RefCell<HostContext<'static>>>, app: &gtk::Application) {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("WebAssembly shared buffers [Rust]")
        .build();

    let drawing_area = gtk::DrawingArea::builder()
        .content_width(GRID_W * SCALE as i32)
        .content_height(GRID_H * SCALE as i32)
        .build();
    {
        let ctx = ctx.clone();
        drawing_area.set_draw_func(move |area, cr, width, height| {
            on_draw(ctx.clone(), area, cr, width, height);
        });
    }

    let host_modify_btn = gtk::Button::with_label("Host modifies grid");
    {
        let ctx = ctx.clone();
        host_modify_btn.connect_clicked(move |_btn| ctx.borrow_mut().toggle_host_modify());
    }

    let container_modify_btn = gtk::Button::with_label("Container modifies grid");
    {
        let ctx = ctx.clone();
        container_modify_btn.connect_clicked(move |_btn| {
            // Container will crash, which will cause host to panic when idle signal is not received.
            ctx.borrow_mut().actors.send_signal(Signal::ModifyGrid, true);
        });
    }

    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    hbox.append(&host_modify_btn);
    hbox.append(&container_modify_btn);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 10);
    vbox.append(&drawing_area);
    vbox.append(&hbox);

    window.set_child(Some(&vbox));
    window.present();
    ctx.borrow_mut().timeout_id.replace({
        let ctx = ctx.clone();
        glib::timeout_add_local(
            Duration::from_millis(TICK_MS),
            move || on_tick(ctx.clone(), &drawing_area)
        )
    });
}

fn on_draw(ctx: Rc<RefCell<HostContext>>, _da: &gtk::DrawingArea, cr: &cairo::Context, width: i32, height: i32) {
    cr.set_source_rgb(1.0, 1.0, 0.95);
    cr.rectangle(0.0, 0.0, width as f64, height as f64);
    cr.fill().unwrap();

    let hc = ctx.borrow();
    for y in 0..GRID_H {
        for x in 0..GRID_W {
            if hc.grid.get(x, y) == 1 {
                cr.set_source_rgb(0.3, 0.3, 0.3);
                cr.rectangle(x as f64 * SCALE, y as f64 * SCALE, SCALE, SCALE);
                cr.fill().unwrap();
            }
        }
    }

    let hunter = hc.actors.hunter();
    cr.set_source_rgb(0.8, 0.5, 0.9);
    cr.rectangle(hunter.x as f64 * SCALE, hunter.y as f64 * SCALE, SCALE, SCALE);
    cr.fill().unwrap();

    const TWO_PI: f64 = 2.0 * 3.141593;
    const HSCALE: f64 = SCALE / 2.0;
    for i in 0..N_RUNNERS {
        let (pos, state) = hc.actors.runner(i);
        match state {
            State::Walking => cr.set_source_rgb(0.5, 0.8, 0.9),
            State::Running => cr.set_source_rgb(1.0, 0.8, 0.5),
            State::Dead => cr.set_source_rgb(1.0, 0.4, 0.4),
        }
        cr.arc(pos.x as f64 * SCALE + HSCALE, pos.y as f64 * SCALE + HSCALE, HSCALE, 0.0, TWO_PI);
        cr.fill().unwrap();
    }
}

fn on_tick(ctx: Rc<RefCell<HostContext>>, area: &gtk::DrawingArea) -> glib::Continue {
    let mut hc = ctx.borrow_mut();
    if hc.enable_host_modify {
        hc.grid.modify();
    }
    hc.actors.send_signal(Signal::Tick, true);
    area.queue_draw();
    glib::Continue(true)
}
