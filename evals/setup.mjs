// Creates a fresh working copy of an eval case for one agent run.
//
//   node evals/setup.mjs <case-id> <dest>
//
// A normal case is a copy of evals/cases/<id>/repo. A PR case is a git repo with two commits: the base
// case's code (plus `base_extra`, e.g. an existing review file), then the PR (evals/cases/<id>/head copied
// over it).
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const [caseId, dest] = process.argv.slice(2);
if (!caseId || !dest) {
  console.error("usage: node evals/setup.mjs <case-id> <dest>");
  process.exit(2);
}
const spec = JSON.parse(readFileSync(path.join(here, "cases", caseId, "case.json"), "utf8"));
if (existsSync(dest)) rmSync(dest, { recursive: true });
mkdirSync(dest, { recursive: true });

const git = (...args) =>
  execFileSync("git", ["-c", "user.name=Dev", "-c", "user.email=dev@example.com", "-c", "commit.gpgsign=false", ...args], {
    cwd: dest,
    stdio: "ignore",
  });

if (spec.pr) {
  cpSync(path.join(here, "cases", spec.pr.base, "repo"), dest, { recursive: true });
  if (spec.pr.base_extra) cpSync(path.join(here, "cases", caseId, spec.pr.base_extra), dest, { recursive: true });
  git("init", "-q", "-b", "main");
  git("add", "-A");
  git("commit", "-q", "-m", "Initial");
  cpSync(path.join(here, "cases", caseId, "head"), dest, { recursive: true });
  git("add", "-A");
  git("commit", "-q", "-m", spec.pr.title);
} else {
  cpSync(path.join(here, "cases", caseId, "repo"), dest, { recursive: true });
  git("init", "-q", "-b", "main");
  git("add", "-A");
  git("commit", "-q", "-m", "Initial");
}
console.log(dest);
