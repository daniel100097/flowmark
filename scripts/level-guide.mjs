// Prints the level guide (SPEC.md §10) as a standalone markdown file for the skill.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const spec = readFileSync(path.join(root, "SPEC.md"), "utf8");
const start = spec.indexOf("## 10. Level guide");
const end = spec.indexOf("\n---\n", start);
if (start < 0 || end < 0) throw new Error("SPEC.md §10 not found");
const body = spec
  .slice(start, end)
  .replace("## 10. Level guide", "# Level guide")
  .replace(/^> This section is self-contained so it can become a writer skill\.\n\n/m, "")
  .replace(/^### 10\.\d+ /gm, "## ");
process.stdout.write(
  "<!-- Generated from SPEC.md §10 by scripts/sync-skill.sh. Do not edit here. -->\n\n" + body.trimEnd() + "\n",
);
