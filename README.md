# Shared memory buffers in WebAssembly

This repository demonstrates the use of the POSIX virtual memory API to map
externally allocated buffers into multiple separate WebAssembly processes.

Three implementations are included: a simple terminal-based one in C and a GTK
"game" in both C and Rust. In all three, a host process creates two shared
memory buffers then forks two container processes which embed a wasm engine to
execute separate wasm modules. The containers map the shared buffers into the
wasm linear memory, one with read-only and one with read-write flags.

The C implementations use
[`wasm-micro-runtime`](https://github.com/bytecodealliance/wasm-micro-runtime)
and the Rust one uses [`wasmi`](https://github.com/paritytech/wasmi).

The terminal implementation performs some basic memory checks and confirms
cross-process interaction via the buffers.

The GTK implementations presents a grid field with actors controlled by the
wasm modules: a "hunter" that chases some "runners". The hunter is controlled
by one wasm instance and the runners by the other. The grid data is held in the
read-only shared buffer (initialised by the host) while the actor coordinates
are in the read-write one.

The GTK interface has a couple of buttons to show the memory protection at work.
One instructs the host process to update the read-only buffer (to which it has
write access) and modify the grid. The other instructs the hunter container
process to do the same, at which point it will crash.
