// Viewer tests in jsdom against real view models from `flowmark model`.
// Run: (cd viewer && npm install && npm test). Builds/uses target/debug/flowmark.
import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { JSDOM } from "jsdom";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../..");
const bin = path.join(root, "target/debug/flowmark");
const EXAMPLE = path.join(root, "examples/register-user.review.ts");

function model(...files) {
  if (!existsSync(bin)) execFileSync("cargo", ["build", "-q", "-p", "flowmark_cli"], { cwd: root });
  return JSON.parse(execFileSync(bin, ["model", ...files]));
}

function mount(m, level = 1) {
  const dom = new JSDOM('<!doctype html><body class="flowmark"><div id="r"></div></body>', {
    url: "http://localhost/",
    runScripts: "outside-only",
  });
  dom.window.scrollTo = () => {};
  dom.window.eval(readFileSync(path.join(here, "../viewer.js"), "utf8"));
  const el = dom.window.document.getElementById("r");
  const viewer = dom.window.Flowmark.mount(el, m, { level });
  return { dom, el, viewer };
}

const code = (el) => [...el.querySelectorAll(".fm-code .row")].map((r) => r.textContent).join("\n");
const click = (node) => node.dispatchEvent(new node.ownerDocument.defaultView.MouseEvent("click", { bubbles: true }));

test("L1 collapses, bubbles risks and shows helper badges at the call site", () => {
  const { el } = mount(model(EXAMPLE), 1);
  const text = code(el);
  assert.match(text, /type User = \{ … 5 fields \}/);
  assert.match(text, /interface Events \{ … 1 event \}/);
  assert.match(text, /! mass-assignable/, "risk of a hidden field is bubbled under the collapsed type");
  assert.match(text, /const user = await createUser\(req\.raw\).*writes User, Address/);
  assert.match(text, /! the listener is queued before commit/, "risk inside the hidden helper is bubbled");
  assert.doesNotMatch(text, /db\.insert<User>/, "helper body is hidden");
  assert.doesNotMatch(text, /comes from the migration/, "? comments are L2");
  assert.doesNotMatch(text, /used by:/);
});

test("clicking a collapsed type expands it", () => {
  const { el } = mount(model(EXAMPLE), 1);
  const row = [...el.querySelectorAll(".row.collapsed")].find((r) => r.textContent.includes("type User"));
  click(row);
  assert.match(code(el), /role: "user" \| "admin"/);
});

test("a hidden helper can be expanded at its call site", () => {
  const { el } = mount(model(EXAMPLE), 1);
  click(el.querySelector(".helper-toggle"));
  const inline = el.querySelector(".fm-inline");
  assert.ok(inline, "inline block rendered");
  assert.match(inline.textContent, /db\.insert<User>/);
  assert.match(inline.textContent, /⋯ 1 line/, "the L3 statement inside stays collapsed");
});

test("L2 and L3", () => {
  const two = code(mount(model(EXAMPLE), 2).el);
  assert.match(two, /db\.insert<User>/);
  assert.match(two, /⋯ 1 line/);
  assert.match(two, /used by:/);
  assert.doesNotMatch(two, /toLowerCase/);
  const three = code(mount(model(EXAMPLE), 3).el);
  assert.match(three, /toLowerCase/);
  assert.doesNotMatch(three, /⋯/);
});

test("L0 shows the card and risks; clicking a risk reveals it", () => {
  const { el, viewer } = mount(model(EXAMPLE), 0);
  assert.equal(el.querySelector(".fm-card h1").textContent, "Register user");
  const [riskList, checkerList] = el.querySelectorAll(".fm-list");
  const risks = [...riskList.querySelectorAll("li")];
  assert.equal(risks.length, 5);
  assert.match(checkerList.textContent, /W060/, "the checker finding is listed too");
  const listener = risks.find((li) => li.textContent.includes("queued before commit"));
  click(listener.querySelector("a"));
  assert.equal(viewer.level, 1);
  const flashed = el.querySelector(".row.flash");
  assert.ok(flashed, "the risk row is highlighted");
  assert.match(flashed.textContent, /queued before commit/);
  assert.match(code(el), /db\.insert<User>/, "its helper was expanded");
});

test("keys 0-3 switch levels", () => {
  const { el, dom, viewer } = mount(model(EXAMPLE), 1);
  dom.window.document.dispatchEvent(new dom.window.KeyboardEvent("keydown", { key: "3" }));
  assert.equal(viewer.level, 3);
  assert.equal(el.querySelector(".fm-levels .active").textContent, "L3 Full");
});

test("ref chips carry target, quote and scope", () => {
  const { el } = mount(model(EXAMPLE), 1);
  const chip = [...el.querySelectorAll(".ref")].find((r) => r.textContent.includes("$request->all()"));
  assert.ok(chip);
  assert.match(chip.title, /searched in: UserController\.store/);
});

test("diff mode", () => {
  const dir = mkdtempSync(path.join(tmpdir(), "fm-"));
  const base = readFileSync(EXAMPLE, "utf8");
  const head = base
    .replace("  access: \"public\",", "  access: \"user\",")
    .replace(
      '    await db.insert<Address>({ ...data.address, userId: user.id }) // @ "->address()->create("\n',
      '    await db.insert<Address>({ ...data.address, userId: user.id }) // @ "->address()->create("\n' +
        "    // ! external call inside the transaction: a rollback does not undo the Stripe customer\n" +
        '    await external("Stripe").createCustomer(email) // @ "Customer::create("\n',
    );
  writeFileSync(path.join(dir, "head.review.ts"), head);
  const m = model(EXAMPLE, path.join(dir, "head.review.ts"));

  const { el, viewer } = mount(m, 1);
  const changes = el.querySelector(".fm-changes").textContent;
  assert.match(changes, /new risk.*external call inside the transaction/);
  assert.match(changes, /entry registerUser changed.*access: "public" → "user"/);
  assert.equal(el.querySelectorAll(".row.removed").length, 1);
  // "show changes" is on: the change inside the hidden helper is visible at L1.
  assert.match(code(el), /external\("Stripe"\)\.createCustomer/);

  viewer.showChanges = false;
  viewer.render();
  assert.doesNotMatch(code(el), /external\("Stripe"\)/);
  const toggle = el.querySelector(".helper-toggle").textContent;
  assert.match(toggle, /calls Stripe\.createCustomer/);
  assert.match(toggle, /Δ2/);

  // Changes | Before | After
  const modes = () => [...el.querySelectorAll(".fm-modes button")];
  assert.deepEqual(modes().map((b) => b.textContent), ["Changes", "Before", "After"]);
  click(modes()[2]);
  assert.equal(viewer.mode, "after");
  assert.equal(el.querySelectorAll(".row.added, .row.removed").length, 0, "After is the head file alone");
  assert.equal(el.querySelector(".fm-changes"), null);
  click(modes()[1]);
  assert.match(el.querySelector(".fm-levels:not(.fm-modes) .active").textContent, /L1/, "the level is kept");
  assert.match(code(el), /access: "public"/, "Before is the base file");
  viewer.key({ key: "d", target: null });
  assert.equal(viewer.mode, "changes");
  assert.ok(el.querySelector(".fm-changes"));
});

test("a single file has no Changes/Before/After switch", () => {
  const { el } = mount(model(EXAMPLE), 1);
  assert.equal(el.querySelector(".fm-modes"), null);
});

test("section headings follow their content", () => {
  const { el, viewer } = mount(model(EXAMPLE), 1);
  const sections = () => [...el.querySelectorAll(".row.section")].map((r) => r.textContent.replace(/\d+/g, "").trim());
  assert.deepEqual(sections(), ["data", "flow", "async"], "details is empty at L1");
  viewer.reveal(el.querySelector("[data-row]") && 65);
  assert.ok(sections().includes("details"), "revealing a row inside the helper shows its section");
});
