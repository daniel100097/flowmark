// The skill is well-formed and ships up-to-date copies of the prelude, the examples and the level guide.
import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const skill = path.join(root, "skills/flowmark-review");
const read = (...p) => readFileSync(path.join(...p), "utf8");
const bin = path.join(root, "target/debug/flowmark");

test("SKILL.md frontmatter", () => {
  const md = read(skill, "SKILL.md");
  const m = /^---\nname: (.+)\ndescription: (.+)\n---\n/.exec(md);
  assert.ok(m, "frontmatter with name and description");
  assert.equal(m[1], "flowmark-review");
  assert.match(m[1], /^[a-z0-9-]+$/);
  assert.ok(m[2].length > 100 && m[2].length <= 1024, `description length ${m[2].length}`);
});

test("files SKILL.md points to exist", () => {
  const md = read(skill, "SKILL.md");
  for (const f of ["prelude.d.ts", "examples/register-user.review.ts", "examples/t3code-update-server-settings.review.ts", "reference/levels.md"]) {
    assert.ok(md.includes(f), `SKILL.md mentions ${f}`);
    assert.ok(existsSync(path.join(skill, f)), `${f} exists`);
  }
});

test("copies are in sync with the repo (run scripts/sync-skill.sh)", () => {
  assert.equal(read(skill, "prelude.d.ts"), read(root, "prelude.d.ts"));
  assert.equal(read(skill, "examples/register-user.review.ts"), read(root, "examples/register-user.review.ts"));
  assert.equal(
    read(skill, "examples/t3code-update-server-settings.review.ts"),
    read(root, "examples/t3code-pr-25709/head.review.ts"),
  );
  const guide = execFileSync("node", [path.join(root, "scripts/level-guide.mjs")]).toString();
  assert.equal(read(skill, "reference/levels.md"), guide);
});

test("the shipped examples pass flowmark check (only the example's known finding)", () => {
  if (!existsSync(bin)) execFileSync("cargo", ["build", "-q", "-p", "flowmark_cli"], { cwd: root });
  const out = (f) => {
    try {
      return execFileSync(bin, ["check", path.join(skill, "examples", f)]).toString();
    } catch (e) {
      return e.stdout.toString() + "EXIT " + e.status;
    }
  };
  assert.match(out("register-user.review.ts"), /^\S+:53: warning W060: [^\n]+\n$/);
  assert.equal(out("t3code-update-server-settings.review.ts"), "");
});
