// The Flowmark web app: parse and check in WebAssembly, render with viewer.js.
//
//   FlowmarkApp.start({
//     wasm: "flowmark.wasm",
//     examples: [{ label: "Register user", src: "examples/register-user.review.ts" },
//                { label: "A PR (diff)", src: "head.review.ts", base: "base.review.ts" }],
//   })
//
// URL parameters: ?src=<url of a .review.ts> &base=<url of the base version> &level=<0-3>
(function (global) {
  "use strict";

  const STORE = "flowmark:last";
  const $ = (id) => document.getElementById(id);

  const app = {
    fm: null,
    viewer: null,
    head: null, // { name, text }
    base: null,
    timer: null,
    level: 1,

    async start(opts) {
      this.opts = opts || {};
      this.examples = this.opts.examples || (this.opts.example ? [{ label: "Example", src: this.opts.example }] : []);
      const q = new URLSearchParams(global.location.search);
      if (q.has("level")) this.level = Math.max(0, Math.min(3, Number(q.get("level")) || 0));
      this.bind();
      try {
        this.fm = await global.FlowmarkWasm.load(this.opts.wasm || "flowmark.wasm");
      } catch (e) {
        return this.status("Could not load flowmark.wasm: " + e.message, true);
      }
      try {
        if (q.has("src")) {
          this.head = await fetchText(q.get("src"));
          if (q.has("base")) this.base = await fetchText(q.get("base"));
        } else {
          const saved = global.localStorage && global.localStorage.getItem(STORE);
          if (saved) this.head = JSON.parse(saved);
          else if (this.examples.length) await this.loadExample(this.examples[0]);
        }
      } catch (e) {
        this.status(e.message, true);
      }
      await this.render();
    },

    bind() {
      $("open").addEventListener("change", async (e) => {
        if (e.target.files[0]) this.head = await readFile(e.target.files[0]);
        e.target.value = "";
        this.render(true);
      });
      $("open-base").addEventListener("change", async (e) => {
        if (e.target.files[0]) this.base = await readFile(e.target.files[0]);
        e.target.value = "";
        this.render();
      });
      $("swap").addEventListener("click", () => {
        [this.base, this.head] = [this.head, this.base];
        this.render(true);
      });
      $("clear-base").addEventListener("click", () => {
        this.base = null;
        this.render();
      });
      $("toggle-editor").addEventListener("click", () => {
        const ed = $("editor");
        ed.hidden = !ed.hidden;
        $("toggle-editor").classList.toggle("on", !ed.hidden);
        if (!ed.hidden) ed.focus({ preventScroll: true });
      });
      $("editor").addEventListener("input", () => {
        if (!this.head) this.head = { name: "untitled.review.ts", text: "" };
        this.head.text = $("editor").value;
        clearTimeout(this.timer);
        this.timer = setTimeout(() => this.render(), 200);
      });
      const menu = $("example");
      this.examples.forEach((ex, i) => menu.append(new Option(ex.label, String(i))));
      menu.addEventListener("change", async () => {
        const ex = this.examples[Number(menu.value)];
        menu.value = "";
        if (!ex) return;
        try {
          await this.loadExample(ex);
          this.render(true);
        } catch (e) {
          this.status(e.message, true);
        }
      });
      // Drag and drop: one file = the file to view; two files = base and head (by name order).
      let depth = 0;
      document.addEventListener("dragenter", (e) => {
        e.preventDefault();
        depth++;
        document.body.classList.add("dragging");
      });
      document.addEventListener("dragleave", () => {
        if (--depth <= 0) document.body.classList.remove("dragging");
      });
      document.addEventListener("dragover", (e) => e.preventDefault());
      document.addEventListener("drop", async (e) => {
        e.preventDefault();
        depth = 0;
        document.body.classList.remove("dragging");
        const files = [...e.dataTransfer.files].sort((a, b) => a.name.localeCompare(b.name));
        if (!files.length) return;
        if (files.length >= 2) {
          this.base = await readFile(files[0]);
          this.head = await readFile(files[1]);
        } else {
          this.head = await readFile(files[0]);
        }
        this.render(true);
      });
    },

    async loadExample(ex) {
      this.head = await fetchText(ex.src);
      this.base = ex.base ? await fetchText(ex.base) : null;
    },

    async render(syncEditor) {
      if (!this.head) return this.status("Open or drop a .review.ts file.");
      if (syncEditor || $("editor").value === "") $("editor").value = this.head.text;
      if (global.localStorage) global.localStorage.setItem(STORE, JSON.stringify(this.head));
      $("swap").hidden = $("clear-base").hidden = !this.base;
      $("files").textContent = this.base ? `${this.base.name} → ${this.head.name}` : this.head.name;
      const t0 = performance.now();
      let model;
      try {
        model = this.base
          ? await this.fm.diffModel(this.base.text, this.head.text, this.base.name, this.head.name)
          : await this.fm.model(this.head.text, this.head.name);
      } catch (e) {
        return this.status(e.message, true);
      }
      const ms = Math.round(performance.now() - t0);
      if (this.viewer) {
        this.level = this.viewer.level;
        this.viewer.destroy();
      }
      this.viewer = global.Flowmark.mount($("viewer"), model, { level: this.level });
      const errors = model.diagnostics.filter((d) => d.severity === "error").length;
      const warnings = model.diagnostics.length - errors;
      const problems = [errors && `${errors} error${errors > 1 ? "s" : ""}`, warnings && `${warnings} warning${warnings > 1 ? "s" : ""}`]
        .filter(Boolean)
        .join(", ");
      this.status(`${problems || "✓ no problems"} · checked in ${ms} ms`, errors > 0);
    },

    status(text, error) {
      const el = $("status");
      el.textContent = text;
      el.classList.toggle("error", !!error);
    },
  };

  async function fetchText(url) {
    const res = await fetch(url);
    if (!res.ok) throw new Error(`Could not load ${url}: ${res.status}`);
    return { name: decodeURIComponent(url.split("/").pop().split("?")[0]) || url, text: await res.text() };
  }

  function readFile(file) {
    return file.text().then((text) => ({ name: file.name, text }));
  }

  global.FlowmarkApp = app;
})(window);
