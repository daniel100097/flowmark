// Grades the review files an agent wrote for one eval case.
//
//   node evals/grade.mjs <case-id> <run-dir> [--json]
//
// <run-dir> is the repo copy the agent worked in. Review files are all *.review.ts in it. Refs are checked
// against the pristine code in evals/cases (for a PR: base refs against base, head refs against head).
// Needs `flowmark` on PATH (or FLOWMARK=/path/to/flowmark) and `npx` for the type check.
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readdirSync, readFileSync, statSync, cpSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "..");
const FLOWMARK = process.env.FLOWMARK || "flowmark";
const [caseId, runDir] = process.argv.slice(2).filter((a) => !a.startsWith("--"));
const asJson = process.argv.includes("--json");
if (!caseId || !runDir) {
  console.error("usage: node evals/grade.mjs <case-id> <run-dir> [--json]");
  process.exit(2);
}
const spec = JSON.parse(readFileSync(path.join(here, "cases", caseId, "case.json"), "utf8"));
const truth = spec.truth;

// ─── helpers ────────────────────────────────────────────────────────────────

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    if (name === "node_modules" || name === ".git") continue;
    const p = path.join(dir, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else out.push(p);
  }
  return out;
}
const fm = (...args) => spawnSync(FLOWMARK, args, { encoding: "utf8" });
const parse = (file) => JSON.parse(fm("parse", file).stdout);
const norm = (s) => s.toLowerCase().replace(/[^a-z0-9]/g, "").replace(/s$/, "");
const squash = (s) => s.replace(/\s+/g, "");
const matchesAll = (text, patterns) => patterns.every((p) => new RegExp(p, "i").test(text));
const found = (texts, rule) => rule.any.some((alt) => texts.some((t) => matchesAll(t, alt)));

function typecheck(file) {
  const r = spawnSync(
    "npx",
    ["-y", "-p", "typescript@5", "tsc", "--noEmit", "--strict", "--noUnusedLocals", "--lib", "es2022", "--target", "es2022", path.join(root, "prelude.d.ts"), file],
    { encoding: "utf8" },
  );
  return (r.stdout.match(/error TS\d+/g) || []).length;
}

/// Source files of a code tree, as [relative path, text].
function codeFiles(dir) {
  return walk(dir)
    .filter((p) => !p.endsWith(".review.ts") && !p.includes(`${path.sep}docs${path.sep}review`))
    .map((p) => [path.relative(dir, p).split(path.sep).join("/"), readFileSync(p, "utf8")]);
}

/// Checks one ref against the code. Returns null when it holds, or why it does not.
function checkRef(ref, files) {
  const target = ref.resolved;
  if (!target) return "no target or scope";
  let candidates = files;
  if (target.path) {
    const p = target.path.replace(/^\.?\//, "");
    candidates = files.filter(([f]) => f === p || f.endsWith("/" + p));
    if (!candidates.length) return `file ${target.path} not found`;
  }
  if (target.symbol) {
    const parts = target.symbol.split(".").filter(Boolean);
    const name = parts[parts.length - 1];
    const owner = parts.length > 1 ? parts[0] : null;
    const word = (w) => new RegExp(`(^|[^A-Za-z0-9_$])${w.replace(/[$]/g, "\\$")}($|[^A-Za-z0-9_$])`);
    const withName = candidates.filter(([, t]) => word(name).test(t));
    const withOwner = owner ? withName.filter(([f, t]) => word(owner).test(t) || f.includes(owner)) : withName;
    if (!withOwner.length) return `symbol ${target.symbol} not found`;
    candidates = withOwner;
  }
  if (ref.quote && !candidates.some(([, t]) => squash(t).includes(squash(ref.quote)))) {
    return `quote "${ref.quote}" not found in ${target.raw}`;
  }
  return null;
}

/// Level rule: an L2/L3 marker must not hide a write or an effect (directly or through a helper).
function levelViolations(doc) {
  const kids = doc.units.map(() => []);
  for (const u of doc.units) if (u.parent != null) kids[u.parent].push(u.id);
  const subtree = (id) => {
    const out = [id];
    for (let i = 0; i < out.length; i++) out.push(...kids[out[i]]);
    return out;
  };
  const acting = (units) => doc.effects.some((e) => units.has(e.unit) && (e.category === "write" || e.category === "effect"));
  const helperActs = new Map(doc.functions.map((f) => [f.name, acting(new Set(subtree(f.unit)))]));
  const out = [];
  for (const u of doc.units) {
    if (u.marker == null || u.marker < 2) continue;
    const set = new Set(subtree(u.id));
    const viaHelper = doc.calls.some((c) => set.has(c.unit) && helperActs.get(c.callee));
    if (acting(set) || viaHelper) out.push(`line ${u.span.start_line}: L${u.marker} hides a write/effect`);
  }
  return out;
}

// ─── collect ────────────────────────────────────────────────────────────────

const reviewFiles = walk(runDir).filter((p) => p.endsWith(".review.ts"));
const result = { case: caseId, files: reviewFiles.map((f) => path.relative(runDir, f)), checks: {}, notes: [] };
if (!reviewFiles.length) {
  result.score = 0;
  result.notes.push("no *.review.ts written");
  report();
  process.exit(0);
}

let headFiles = reviewFiles;
let baseFiles = [];
let codeFor = () => codeFiles(path.join(here, "cases", caseId, "repo"));
if (spec.pr) {
  baseFiles = reviewFiles.filter((f) => /base/i.test(path.basename(f)));
  headFiles = reviewFiles.filter((f) => !baseFiles.includes(f));
  const baseCode = path.join(here, "cases", spec.pr.base, "repo");
  const headCode = mkdtempSync(path.join(tmpdir(), "fm-head-"));
  cpSync(baseCode, headCode, { recursive: true });
  cpSync(path.join(here, "cases", caseId, "head"), headCode, { recursive: true });
  const base = codeFiles(baseCode);
  const head = codeFiles(headCode);
  codeFor = (file) => (baseFiles.includes(file) ? base : head);
}

const docs = reviewFiles.map((file) => ({ file, doc: parse(file) }));
const headDocs = docs.filter((d) => headFiles.includes(d.file)).map((d) => d.doc);

// validity
let errors = 0;
let warnings = 0;
let tscErrors = 0;
for (const { file, doc } of docs) {
  errors += doc.diagnostics.filter((d) => d.severity === "error").length;
  warnings += doc.diagnostics.filter((d) => d.severity === "warning").length;
  tscErrors += typecheck(file);
}
result.checks.valid = { errors, warnings, tscErrors, ok: errors === 0 && tscErrors === 0 };

// entries and access (head side)
const entries = headDocs.flatMap((d) => d.entries);
result.checks.entries = truth.entries.map((t) => {
  const e = entries.find((x) => x.trigger && new RegExp(t.trigger, "i").test(x.trigger.raw));
  const accessOk = !t.access || (e && (e.access || "").toLowerCase().includes(t.access));
  return { trigger: t.trigger, found: !!e, accessOk: !!(e && accessOk), access: e ? e.access : null };
});

// input shapes
if (truth.input) {
  const inputs = entries.map((e) => e.input).filter(Boolean);
  const validated = headDocs.flatMap((d) => d.effects.filter((x) => x.call === "validate").map((x) => x.name || ""));
  result.checks.input = truth.input.map((t) => ({
    expected: t,
    found: [...inputs, ...validated].some((i) => t.split("|").some((alt) => norm(i).includes(norm(alt)))),
  }));
}

// writes and effects: from the body and the card
const cardItems = (field) =>
  headDocs.flatMap((d) => (d.card ? d.card.fields.filter((f) => f.name === field).flatMap((f) => f.items) : []));
const writeNames = [
  ...headDocs.flatMap((d) => d.effects.filter((e) => e.category === "write").map((e) => e.name || "")),
  ...cardItems("writes").map((i) => i.name),
];
const effectNames = [
  ...headDocs.flatMap((d) => d.effects.filter((e) => e.category === "effect").map((e) => `${e.call} ${e.name || ""}`)),
  ...cardItems("effects").map((i) => `${i.kind || ""} ${i.name}`),
];
const hasName = (names, alts) => names.some((n) => alts.split("|").some((a) => norm(n).includes(norm(a)) || (norm(n) && norm(a).includes(norm(n)))));
result.checks.writes = truth.writes.map((w) => ({ expected: w, found: hasName(writeNames, w) }));
result.checks.effects = truth.effects.map((w) => ({
  expected: w,
  found: effectNames.some((n) => new RegExp(w, "i").test(n)),
}));

// planted risks (head side)
const riskTexts = headDocs.flatMap((d) => d.risks.map((r) => `${r.kind} ${r.text}`));
result.checks.risks = truth.risks.map((r) => ({ id: r.id, found: found(riskTexts, r) }));
result.checks.bonusRisks = (truth.bonus_risks || []).map((r) => ({ id: r.id, found: found(riskTexts, r) }));
result.checks.riskCount = riskTexts.length;

// refs
let refTotal = 0;
const refFailures = [];
for (const { file, doc } of docs) {
  const code = codeFor(file);
  for (const ref of doc.refs) {
    refTotal++;
    const why = checkRef(ref, code);
    if (why) refFailures.push(`${path.basename(file)}:${ref.line} ${ref.raw} → ${why}`);
  }
}
result.checks.refs = { total: refTotal, verified: refTotal - refFailures.length, failures: refFailures };

// level rule
result.checks.levels = docs.flatMap(({ file, doc }) => levelViolations(doc).map((v) => `${path.basename(file)} ${v}`));

// size
result.checks.size = docs.map(({ file }) => ({
  file: path.basename(file),
  lines: readFileSync(file, "utf8").split("\n").length,
  l1Lines: fm("view", file, "--level", "1").stdout.split("\n").filter((l) => l.trim()).length,
}));

// PR
if (spec.pr) {
  const pr = { baseFile: baseFiles.map((f) => path.basename(f)), headFile: headFiles.map((f) => path.basename(f)) };
  if (baseFiles.length === 1 && headFiles.length === 1) {
    const m = JSON.parse(fm("model", baseFiles[0], headFiles[0]).stdout);
    const c = m.changes;
    pr.risksAdded = truth.pr.risks_added.map((r) => ({ id: r.id, found: found(c.risks_added, r) }));
    pr.risksRemoved = truth.pr.risks_removed.map((r) => ({ id: r.id, found: found(c.risks_removed, r) }));
    pr.cardChanged = truth.pr.card_changed.map((f) => ({ field: f, found: c.card.some((x) => x.name === f) }));
    const baseLines = readFileSync(baseFiles[0], "utf8").split("\n").length;
    pr.diff = { added: c.lines_added, removed: c.lines_removed, baseLines, changedRatio: +(c.lines_removed / baseLines).toFixed(2) };
  } else {
    result.notes.push("PR: expected exactly one base and one head file");
  }
  result.checks.pr = pr;
}

// ─── score (0–100) ──────────────────────────────────────────────────────────

const frac = (list, key = "found") => (list && list.length ? list.filter((x) => x[key]).length / list.length : 1);
const parts = {
  valid: [15, result.checks.valid.ok ? 1 : 0],
  entries: [10, frac(result.checks.entries)],
  access: [5, frac(result.checks.entries, "accessOk")],
  input: [5, frac(result.checks.input)],
  writes: [10, frac(result.checks.writes)],
  effects: [10, frac(result.checks.effects)],
  risks: [30, frac(result.checks.risks)],
  refs: [15, refTotal ? (refTotal - refFailures.length) / refTotal : 0],
};
if (spec.pr && result.checks.pr.risksAdded) {
  const pr = result.checks.pr;
  parts.prRisks = [15, (frac(pr.risksAdded) + frac(pr.risksRemoved)) / 2];
  parts.prCard = [5, frac(pr.cardChanged)];
  parts.prMinimal = [5, pr.diff.changedRatio <= 0.3 ? 1 : pr.diff.changedRatio <= 0.6 ? 0.5 : 0];
}
const max = Object.values(parts).reduce((s, [w]) => s + w, 0);
const got = Object.values(parts).reduce((s, [w, f]) => s + w * f, 0);
const penalty = Math.min(10, 5 * result.checks.levels.length);
result.parts = Object.fromEntries(Object.entries(parts).map(([k, [w, f]]) => [k, `${Math.round(w * f)}/${w}`]));
result.score = Math.max(0, Math.round((100 * got) / max) - penalty);
report();

function report() {
  if (asJson) return console.log(JSON.stringify(result, null, 2));
  const c = result.checks;
  console.log(`${caseId}: ${result.score}/100  (${result.files.join(", ")})`);
  if (!c.valid) return result.notes.forEach((n) => console.log("  " + n));
  console.log(`  parts: ${Object.entries(result.parts).map(([k, v]) => `${k} ${v}`).join(", ")}`);
  console.log(`  valid: ${c.valid.errors} errors, ${c.valid.warnings} warnings, ${c.valid.tscErrors} tsc errors`);
  console.log(`  entries: ${c.entries.map((e) => `${e.trigger} ${e.found ? (e.accessOk ? "✓" : `access ✗ (${e.access})`) : "✗"}`).join("; ")}`);
  if (c.input) console.log(`  input: ${c.input.map((i) => `${i.expected} ${i.found ? "✓" : "✗"}`).join("; ")}`);
  console.log(`  writes: ${c.writes.map((i) => `${i.expected} ${i.found ? "✓" : "✗"}`).join("; ")}`);
  console.log(`  effects: ${c.effects.map((i) => `${i.expected} ${i.found ? "✓" : "✗"}`).join("; ")}`);
  console.log(`  planted risks: ${c.risks.map((r) => `${r.id} ${r.found ? "✓" : "✗"}`).join("; ")}  (${c.riskCount} risks total)`);
  if (c.bonusRisks.length) console.log(`  bonus risks: ${c.bonusRisks.map((r) => `${r.id} ${r.found ? "✓" : "✗"}`).join("; ")}`);
  console.log(`  refs: ${c.refs.verified}/${c.refs.total} verified`);
  for (const f of c.refs.failures) console.log(`    ✗ ${f}`);
  if (c.levels.length) console.log(`  level rule: ${c.levels.join("; ")}`);
  console.log(`  size: ${c.size.map((s) => `${s.file} ${s.lines} lines (L1 ${s.l1Lines})`).join("; ")}`);
  if (c.pr) {
    const pr = c.pr;
    if (pr.risksAdded) {
      console.log(`  PR risks added: ${pr.risksAdded.map((r) => `${r.id} ${r.found ? "✓" : "✗"}`).join("; ")}`);
      console.log(`  PR risks removed: ${pr.risksRemoved.map((r) => `${r.id} ${r.found ? "✓" : "✗"}`).join("; ")}`);
      console.log(`  PR card changed: ${pr.cardChanged.map((r) => `${r.field} ${r.found ? "✓" : "✗"}`).join("; ")}`);
      console.log(`  PR diff: +${pr.diff.added} −${pr.diff.removed} of ${pr.diff.baseLines} base lines (ratio ${pr.diff.changedRatio})`);
    }
  }
  for (const n of result.notes) console.log("  note: " + n);
}
