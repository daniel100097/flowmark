// Flowmark parser/checker/view model in WebAssembly (crates/flowmark_wasm). No dependencies.
//
//   const fm = await FlowmarkWasm.load("flowmark.wasm");   // URL, ArrayBuffer/Uint8Array or WebAssembly.Module
//   await fm.check(source)                                  // → diagnostics
//   await fm.parse(source)                                  // → document
//   await fm.model(source, "file.review.ts")                // → view model for viewer.js
//   await fm.diffModel(base, head, "base.ts", "head.ts")    // → view model in diff mode
//   await fm.view(source, 1)                                // → text view
//
// The parser never frees its syntax trees, so a fresh instance is created once memory passes `maxMemory`.
// A crash (trap) inside the module also gets a fresh instance; the call rejects with the error.
(function (global) {
  "use strict";

  const MODEL_VERSION = 1;
  const enc = new TextEncoder();
  const dec = new TextDecoder();

  async function compile(input) {
    if (input instanceof WebAssembly.Module) return input;
    if (input instanceof ArrayBuffer || ArrayBuffer.isView(input)) return WebAssembly.compile(input);
    const url = input || "flowmark.wasm";
    if (WebAssembly.compileStreaming) {
      try {
        return await WebAssembly.compileStreaming(fetch(url));
      } catch (e) {
        // Wrong MIME type from a simple static server: fall back to a plain fetch.
      }
    }
    const res = await fetch(url);
    if (!res.ok) throw new Error(`flowmark: cannot load ${url}: ${res.status}`);
    return WebAssembly.compile(await res.arrayBuffer());
  }

  class Flowmark {
    constructor(module, options) {
      this.module = module;
      this.maxMemory = (options && options.maxMemory) || 256 * 1024 * 1024;
      this.instances = 0;
      this.w = null;
    }

    async instance() {
      if (!this.w || this.w.memory.buffer.byteLength > this.maxMemory) {
        const inst = await WebAssembly.instantiate(this.module, {});
        this.w = inst.exports;
        this.instances++;
        const v = this.w.fm_model_version();
        if (v !== MODEL_VERSION) throw new Error(`flowmark: wasm model version ${v}, expected ${MODEL_VERSION}`);
      }
      return this.w;
    }

    async call(fn, ...args) {
      const w = await this.instance();
      const flat = [];
      for (const a of args) {
        if (typeof a === "number") {
          flat.push(a);
          continue;
        }
        const bytes = enc.encode(String(a));
        const ptr = w.fm_alloc(bytes.length);
        new Uint8Array(w.memory.buffer, ptr, bytes.length).set(bytes);
        flat.push(ptr, bytes.length);
      }
      try {
        const n = w[fn](...flat);
        return dec.decode(new Uint8Array(w.memory.buffer, w.fm_result_ptr(), n));
      } catch (e) {
        this.w = null; // a trap leaves the instance unusable
        throw new Error(`flowmark: ${fn} failed: ${e && e.message ? e.message : e}`);
      }
    }

    async parse(source) {
      return JSON.parse(await this.call("fm_parse", source));
    }
    async check(source) {
      return JSON.parse(await this.call("fm_check", source));
    }
    async model(source, file) {
      return JSON.parse(await this.call("fm_model", source, file || "untitled.review.ts"));
    }
    async diffModel(base, head, baseFile, headFile) {
      return JSON.parse(
        await this.call("fm_diff_model", base, baseFile || "base.review.ts", head, headFile || "head.review.ts"),
      );
    }
    async view(source, level) {
      return this.call("fm_view", source, level == null ? 1 : level);
    }
  }

  const api = {
    MODEL_VERSION,
    async load(input, options) {
      return new Flowmark(await compile(input), options);
    },
  };
  if (typeof module === "object" && module.exports) module.exports = api;
  global.FlowmarkWasm = api;
})(typeof window !== "undefined" ? window : globalThis);
