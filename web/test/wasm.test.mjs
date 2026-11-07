// The WebAssembly build gives exactly what the native CLI gives, and the web app works with it.
// Run `./scripts/build-web.sh` first (npm test does not build Rust).
import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { JSDOM } from "jsdom";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../..");
const dist = path.join(root, "web/dist");
const bin = path.join(root, "target/debug/flowmark");
const EXAMPLE = path.join(root, "examples/register-user.review.ts");
const source = readFileSync(EXAMPLE, "utf8");

const require = createRequire(import.meta.url);
const FlowmarkWasm = require("../flowmark-wasm.js");
const wasmBytes = () => readFileSync(path.join(dist, "flowmark.wasm"));

function cli(...args) {
  if (!existsSync(bin)) execFileSync("cargo", ["build", "-q", "-p", "flowmark_cli"], { cwd: root });
  return execFileSync(bin, args).toString();
}

test("wasm view model equals the CLI's", async () => {
  const fm = await FlowmarkWasm.load(wasmBytes());
  const wasm = await fm.model(source, EXAMPLE);
  assert.deepEqual(wasm, JSON.parse(cli("model", EXAMPLE)));
});

test("wasm diff model equals the CLI's", async () => {
  const dir = mkdtempSync(path.join(tmpdir(), "fm-"));
  const headFile = path.join(dir, "head.review.ts");
  const head = source.replace('  access: "public",', '  access: "user",');
  writeFileSync(headFile, head);
  const fm = await FlowmarkWasm.load(wasmBytes());
  const wasm = await fm.diffModel(source, head, EXAMPLE, headFile);
  assert.deepEqual(wasm, JSON.parse(cli("model", EXAMPLE, headFile)));
  assert.equal(wasm.changes.entries[0].details[0], 'access: "public" → "user"');
});

test("wasm check, parse and text view", async () => {
  const fm = await FlowmarkWasm.load(wasmBytes());
  // The example's one known finding: declared input, but req.raw is passed on.
  assert.deepEqual((await fm.check(source)).map((d) => [d.code, d.line]), [["W060", 53]]);
  const broken = await fm.check("const x = entry({ trigger: \"FETCH /x\" })\n");
  const codes = broken.map((d) => d.code);
  for (const c of ["E001", "E002", "E021", "E022"]) assert.ok(codes.includes(c), `${c} in ${codes}`);
  const doc = await fm.parse(source);
  assert.equal(doc.entries.length, 2);
  assert.equal(await fm.view(source, 1), cli("view", EXAMPLE, "--level", "1"));
});

test("a fresh instance replaces one that grew too large", async () => {
  const fm = await FlowmarkWasm.load(wasmBytes(), { maxMemory: 1 });
  for (let i = 0; i < 3; i++) assert.equal((await fm.model(source, "x")).rows.length, 82);
  assert.equal(fm.instances, 3);
});

test("the web app loads, checks as you type and shows diagnostics", async () => {
  const html = readFileSync(path.join(dist, "index.html"), "utf8").replace(/<script src="[^"]+"><\/script>\n/g, "");
  const dom = new JSDOM(html.replace(/<script>FlowmarkApp[^<]*<\/script>/, ""), {
    url: "http://localhost/",
    runScripts: "outside-only",
  });
  const w = dom.window;
  w.scrollTo = () => {};
  w.TextEncoder = TextEncoder;
  w.TextDecoder = TextDecoder;
  w.fetch = async (url) => {
    const file = path.join(dist, String(url));
    return existsSync(file)
      ? new Response(readFileSync(file), { headers: { "content-type": url.endsWith(".wasm") ? "application/wasm" : "text/plain" } })
      : new Response("not found", { status: 404 });
  };
  for (const f of ["viewer.js", "flowmark-wasm.js", "app.js"]) w.eval(readFileSync(path.join(dist, f), "utf8"));
  await w.FlowmarkApp.start({
    wasm: wasmBytes(),
    examples: [
      { label: "Register user", src: "examples/register-user.review.ts" },
      { label: "t3code", src: "examples/t3code-pr-25709/head.review.ts", base: "examples/t3code-pr-25709/base.review.ts" },
    ],
  });

  const doc = w.document;
  assert.match(doc.getElementById("status").textContent, /^1 warning ·/);
  assert.match(doc.querySelector(".fm-code").textContent, /const registerUser = entry/);

  // Edit: remove `access` from an entry.
  const editor = doc.getElementById("editor");
  editor.value = source.replace('  access: "public",\n', "");
  editor.dispatchEvent(new w.Event("input"));
  await new Promise((r) => setTimeout(r, 300));
  assert.match(doc.getElementById("status").textContent, /^1 error, 1 warning ·/);
  assert.equal(doc.querySelector(".fm-count.error").textContent.trim(), "1 ✗ 1 ⚠");

  // The examples menu loads the t3code PR as a diff.
  const menu = doc.getElementById("example");
  assert.deepEqual([...menu.options].map((o) => o.textContent), ["Examples…", "Register user", "t3code"]);
  menu.value = "1";
  menu.dispatchEvent(new w.Event("change"));
  await new Promise((r) => setTimeout(r, 300));
  assert.match(doc.getElementById("files").textContent, /base\.review\.ts → head\.review\.ts/);
  assert.ok(doc.querySelector(".fm-changes"), "diff summary shown");
  assert.equal(doc.querySelectorAll(".fm-modes button").length, 3);
});
