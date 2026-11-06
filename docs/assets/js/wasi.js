// SPDX-License-Identifier: MIT
// The five functions of WASI preview 1 the calculators of the rulebook import – reading standard input from a
// string, writing standard output into one, an empty environment, and exit – so that the library, compiled to
// wasm32-wasip1, runs in the browser without any other code.

class Exit {
  constructor(code) {
    this.code = code;
  }
}

/** Runs a compiled WASI command with `input` on its standard input; returns its standard output. */
export async function runWasi(module, input) {
  const stdin = new TextEncoder().encode(input);
  let offset = 0;
  let out = "";
  let memory;
  const view = () => new DataView(memory.buffer);
  const imports = {
    environ_sizes_get(count, size) {
      view().setUint32(count, 0, true);
      view().setUint32(size, 0, true);
      return 0;
    },
    environ_get() {
      return 0;
    },
    fd_read(fd, iovs, length, read) {
      let total = 0;
      for (let i = 0; i < length; i++) {
        const pointer = view().getUint32(iovs + 8 * i, true);
        const capacity = view().getUint32(iovs + 8 * i + 4, true);
        const chunk = fd === 0 ? stdin.subarray(offset, offset + capacity) : new Uint8Array(0);
        new Uint8Array(memory.buffer, pointer, chunk.length).set(chunk);
        offset += chunk.length;
        total += chunk.length;
        if (chunk.length < capacity) break;
      }
      view().setUint32(read, total, true);
      return 0;
    },
    fd_write(fd, iovs, length, written) {
      let total = 0;
      for (let i = 0; i < length; i++) {
        const pointer = view().getUint32(iovs + 8 * i, true);
        const count = view().getUint32(iovs + 8 * i + 4, true);
        if (fd === 1) out += new TextDecoder().decode(new Uint8Array(memory.buffer, pointer, count));
        total += count;
      }
      view().setUint32(written, total, true);
      return 0;
    },
    proc_exit(code) {
      throw new Exit(code);
    },
  };
  const instance = await WebAssembly.instantiate(module, { wasi_snapshot_preview1: imports });
  memory = instance.exports.memory;
  try {
    instance.exports._start();
  } catch (error) {
    if (!(error instanceof Exit)) throw error;
  }
  return out;
}
