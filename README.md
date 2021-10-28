# Shared memory buffers in WebAssembly

This repository demonstrates the use of the POSIX virtual memory API to map
externally allocated buffers into multiple separate WebAssembly processes.

Two implementations are included: a simple terminal-based one and a GTK "game".
In both cases, a host process creates two shared memory buffers then forks two
container processes which embed the
[`wasm-micro-runtime`](https://github.com/bytecodealliance/wasm-micro-runtime)
engine to execute separate wasm modules. The containers map the shared buffers
into the wasm linear memory, one with read-only and one with read-write flags.

The terminal implementation performs some basic memory checks and confirms
cross-process interaction via the buffers.

The GTK implementation presents a grid field with actors controlled by the
wasm modules: a "hunter" that chases some "runners". The hunter is controlled
by one wasm instance and the runners by the other. The grid data is held in the
read-only shared buffer (initialised by the host) while the actor coordinates
are in the read-write one.

The GTK interface has a couple of buttons to show the memory protection at work.
One instructs the host process to update the read-only buffer (to which it has
write access) and modify the grid. The other instructs the hunter container
process to do the same, at which point it will crash.
