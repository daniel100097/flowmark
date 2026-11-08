# Flowmark — Spec v0.1 (draft)

> Working name. A code review and documentation language: AI-written,
> human-read pseudo-TypeScript that explains what code does, links every
> claim to the real code, and shows what needs a human's attention.

Files in this draft:

- `SPEC.md` — this document
- `prelude.d.ts` — the fixed vocabulary (§6)
- `examples/register-user.review.ts` — full example (§14)

---

## 1. Purpose

A Flowmark file transfers knowledge about code to a human reviewer:

- **what** the code does (intent, flow, data),
- **where** each part lives (links to the real code),
- **what needs attention** (risks, uncertainty),
- **what changed** (git diff of the file).

It is written by an AI. It is **not** generated deterministically from the
code and it is **not** written by hand. Because an AI writes it, every claim
must cite code, and a checker verifies the citations (§12).

It is language-neutral: the code under review can be PHP, Go, Python, Java, …
The Flowmark file is always pseudo-TypeScript.

## 2. Principles

1. **Readable as plain text.** On GitHub, without any viewer, the file still makes sense.
2. **Pseudocode, not translation.** One line is one idea, not one line of the original.
3. **Every claim cites code.** Links point to symbols and quotes, never to line numbers.
4. **Hide, never lose.** Detail levels hide lines. They never hide a risk, and a hidden block always says what is inside it (§9).
5. **Diff-stable.** The same code gives the same file; a small code change gives a small diff (§11).
6. **Say "unsure" rather than guess.** The writer marks uncertainty (`?`) instead of inventing.

## 3. Files

| Item | Rule |
|---|---|
| Extension | `*.review.ts` |
| Location (repo mode) | `docs/review/<capability>.review.ts` (configurable) |
| Generated mode | Same format, produced on demand and never committed (§11.4) |
| First line | `// flowmark 0.1` — version marker, also used to detect Flowmark blocks |
| Scope | One file per **capability** ("Register user", "Refund order"). A file may hold several entry points that belong together (an HTTP endpoint, its event listener, its job). Unrelated entry points go in separate files. |
| Imports/exports | None. A file is a script that only sees `prelude.d.ts` and itself. |
| In Markdown | A ```` ```ts ```` fence whose first line is `// flowmark 0.1`. GitHub highlights it as TypeScript; a Flowmark viewer replaces it with the interactive view. |

## 4. File layout

Fixed order. Section markers are line comments of the form `// --- <name> ---`.
Empty sections are left out.

```
// flowmark 0.1
/** review card */           §5
// --- data ---              types: inputs, models, outputs, Events, Jobs
// --- flow ---              entries started from outside or by time: HTTP, schedule, cli, message, …
// --- details ---           helper functions called from the entries
// --- async ---             entries started by this app: event listeners, jobs
```

Inside a section, order follows the code's execution order where there is one;
otherwise the order of first use. Keep the existing order when updating a file.

## 5. Review card

The first block of every file. It answers "Do I need to look, and where?"

```ts
/**
 * <Title>
 * <1–3 lines: what this does and why it exists>
 *
 * access:   ...
 * input:    ...
 * reads:    ...
 * writes:   ...
 * effects:  ...
 * output:   ...
 * config:   ...
 * shared:   ...
 * tests:    ...
 */
```

| Field | Required | Content |
|---|---|---|
| `access` | yes | The `access` of every entry. One value when all entries share it; otherwise one item per value with entry names: `public (registerUser), system (sendWelcomeMail)`. |
| `input` | yes | The entries' `input` shapes and `validate<T>` types; `raw request` if unvalidated input is used. |
| `reads` | no | Models/stores read, when it matters (authorization, locking, cost). |
| `writes` | yes | Models, files and cache keys written. Note transactions in `( )`. |
| `effects` | yes | `event X`, `queue X`, `mail X`, `notify X`, `external Service.op`. |
| `output` | yes | Status + type, e.g. `201 UserResponse`. |
| `config` | no | Feature flags and config keys that change behaviour. |
| `shared` | no | Code in this file that other flows also use; contracts others depend on. |
| `tests` | no | What is covered, in a few words. |

Rules:

- **Fixed field order.** Required fields are always present; write `none` when empty.
  Then a diff shows when something appears for the first time.
- **Items** are separated by `,` or `→` (`→` = "leads to"). An item may end with
  a note in `( )` and/or `!` (needs attention).
- Field names are padded to the same width (`access:   `). This is the only
  alignment allowed in a file (§11.1).
- **The checker compares card and body** (§12.2). A card that contradicts the body is an error.
- Risks are **not** listed in the card. The viewer collects every `!` in the
  file and shows them under the card (§9.5).

## 6. Body: pseudo-TypeScript

The body uses only the prelude (`prelude.d.ts`) and the functions and types
defined in the same file.

### 6.1 Entry points

Every place where execution starts is one `entry()` call, assigned to a `const`
with a domain name:

```ts
const registerUser = entry({
  trigger: "POST /api/users", // @ routes/api.php "Route::post('/users'"
  access: "public",
  input: shape<SignupInput>(), // @ StoreUserRequest.rules
  rateLimit: "10/min", // @ routes/api.php "throttle:10,1"
  async handle(req): Promise<UserResponse> { // @ UserController.store
    const user = await createUser(req.input)
    …
  },
})
```

**Trigger.** One string, `"<kind> <name>"`. The kind decides what `handle` receives
(plus `input`, when the entry declares one):

| Kind | Trigger | `handle` receives |
|---|---|---|
| HTTP, webhook | `"POST /api/users"` | `HttpRequest` |
| App event | `"event UserRegistered"` | the payload declared in `Events` |
| Queued job | `"job SendInvoice"` | the payload declared in `Jobs` |
| Broker message | `"message orders.created"` | `MessageInput` (body is `Raw`) |
| Time | `"schedule daily 03:00"` | nothing |
| Console | `"cli users:import"` | `CliInput` |
| GraphQL | `"query user"`, `"mutation createUser"`, `"subscription …"` | `CallInput` |
| RPC | `"rpc UserService.Create"` | `CallInput` |
| WebSocket | `"ws chat.message"` | `CallInput` |

**Options.** One per line, in this order:

| Option | For | Required | Default level |
|---|---|---|---|
| `trigger` | all | yes | L1, cannot be lowered |
| `access` | all | yes; `"system"` when nobody outside can start it | L1, cannot be lowered |
| `input` | HTTP, GraphQL, RPC, WebSocket, message, cli | when the input is validated before `handle` runs | L1, cannot be lowered |
| `queued` | events | yes | L1 |
| `rateLimit` | HTTP, GraphQL, RPC, WebSocket | no | L1 |
| `retries`, `idempotent`, `unique` | events, jobs, messages | no | L1 |
| `overlap`, `single` | schedule | no | L1 |
| `guards`, `timeout` | all | no | L2 |
| `backoff` | events, jobs, messages | no | L2 |
| `handle` | all | yes | L1, and so are the statements directly in it |

Rules:

- **One fact per line.** A change to `trigger` or `access` is then a one-line diff that nobody can miss.
- **Refs on the line they prove.** The `trigger` line links to where the trigger is
  registered (routes file, listener map, scheduler, consumer config). The `handle`
  line links to the handler and is the ref scope for its body (§8.3).
- **Domain names.** Name the const for what it does (`registerUser`, `sendWelcomeMail`).
  Viewers use the name in the risk list and in cross-references (`used by: registerUser`);
  git shows the line in diff hunk headers.
- **Webhooks** are HTTP entries with `access: { signature: "Stripe" }`.
- **Validated input is part of the entry.** When the framework validates the input before the
  handler runs (form requests, DTO pipes, schema-decoded RPC payloads, ...), declare it:
  `input: shape<SignupInput>()`. Invalid input never reaches `handle` (HTTP: 422). `handle` then
  reads `req.input`, typed by the shape. A separate `validate()` line would only repeat the type
  and suggest the check happens inside the handler.
- **Unvalidated input is always `Raw`** (`req.raw`, `req.params`, `msg.raw`, `call.raw`, `cli.args`).
  An entry that declares `input` but reads `req.raw` gets a warning (§12.3).
- **`validate<T>()` is for validation that is a step in the flow:** in the middle of the
  handler, of something other than the entry's input (a webhook body, a file, an external
  API's answer), or of only part of the input.
- **Type-checked.** The prelude types make `tsc` catch an unknown trigger kind, an unknown
  event or job name, a missing `access`, a missing `queued` on an event, options that don't
  fit the trigger kind (including `input` on events, jobs and schedules), payload fields that
  don't exist, and `req.input` on an entry without `input`.

### 6.2 Writing rules for the body

- **Data goes through types.** Inputs, models, outputs, events and jobs are TypeScript
  types. Use `Pick` / `omit` so exposed fields are visible. Use `Raw` for unvalidated data.
- **Writes and effects go through the prelude** (`db.insert<User>(…)`, `mail(…)`,
  `external("Stripe").charge(…)`), so viewers and the checker can find them.
- **Domain names.** Helpers are named for what they do in the domain (`createUser`,
  `chargeOrder`), not after the real class or method. The real names go in refs.
- **Opaque helpers.** A pure helper whose body doesn't matter to the review (a merge, a mapping,
  a redaction) can be declared instead of written out:
  `declare function redactSecrets(settings: ServerSettings): ServerSettings // @ redactServerSettingsForClient`.
  It is a helper like any other: L2 by default, call sites show its badges, and its own ref scopes the
  comments attached to it. Helpers that write or have effects are always written out.
- **Framework terms only in comments.** `$fillable`, `FormRequest`, `@Transactional`, …
  appear in comments, never as code.
- **One statement per line.** Long calls stay on one line; the viewer wraps.
- **Leave out:** logging (unless it has personal data or is an audit trail),
  dependency-injection wiring, getters and setters, framework glue, plain
  type conversions, metrics.

## 7. Comments

Comments carry links, risks, uncertainty, levels and explanations.

### 7.1 Grammar

```
comment   = "//" [ level ] [ sigil | tag ] text
level     = "L1" | "L2" | "L3"                 (§9)
sigil     = "!" | "!?" | "?"                   followed by a space
tag       = "tested:" | "used by:"
text      = free text, may contain refs anywhere
ref       = "@" SPACE [ target ] [ SPACE quote ]   (§8)
```

| Comment | Meaning |
|---|---|
| `// text` | explanation |
| `// @ X.y` | link (may stand alone or appear inside any comment) |
| `// ! text` | **risk**: needs the reviewer's attention (bug, security, data loss, money, privacy, double execution, irreversible effect) |
| `// !? text` | **possible risk**: the writer is unsure; treated like `!` |
| `// ? text` | **unsure**: the writer could not confirm this; check it |
| `// tested: @ …` | the test that covers the next statement |
| `// used by: … @ …` | other callers of the next function |
| `// L2 …` / `// L3 …` | detail-level marker (§9.3) |

A `!`, `!?` or `?` comment that needs more than one line continues on the following plain `//` lines
(same indentation, no sigil); they are part of the same risk.

Name what goes wrong, not the category: "can be sent twice after a retry", "a client can set role=admin".
That a mail or a charge cannot be undone is not a risk by itself.

A ref starts with `@` followed by a space, at the start of a comment or after a space.
`a@b.com` or `@param` are not refs.

### 7.2 What a comment is attached to

1. **End-of-line comment** → the statement on that line. If that statement opens a
   block (function, callback, `if`, type body), the comment applies to the whole block.
2. **Own-line comments directly above a statement** (no blank line between) → that statement.
3. **Own-line comments at the end of a block** (no statement follows) → the enclosing block.

Write a comment **above** the statement it explains, not below it.

## 8. References

### 8.1 Forms

| Form | Example | Meaning |
|---|---|---|
| Symbol | `@ UserService.create` | definition of a class/function/method, found by name |
| Path + symbol | `@ app/Services/UserService.php#create` | when the symbol name is ambiguous |
| Path | `@ app/Models/User.php` | whole file |
| Symbol + quote | `@ UserController.store "$request->all()"` | exact spot inside the symbol |
| Quote only | `@ "User::create("` | spot inside the **enclosing ref scope** (§8.3) |

- Symbol separators `.`, `::`, `->` are all accepted (`UserService::create` = `UserService.create`).
  The canonical form is `.`. `#` separates a path from a symbol.
- A target that contains `/` or ends in a file extension is a path.
- A path may be a unique suffix: `UserService.php` matches `app/Services/UserService.php`.
- One comment may hold several refs: `// @ A.b @ C.d`.
- Inside a quote, write `\"` for a double quote and `\\` for a backslash:
  `@ "key === \"secretProviders\""`.
- **Never line numbers.** The checker computes lines.

### 8.2 Resolution (language-neutral)

1. Find symbol definitions with tree-sitter tag queries (the same approach as
   GitHub code navigation). Fall back to text search for the identifier.
2. If a symbol name has several definitions → error; the writer must add a path.
3. A quote must appear inside the resolved symbol's body (or file, for a path ref).
   Matching ignores differences in whitespace. If the quote appears several times,
   the ref points to the first match; add more text to the quote to choose another.

### 8.3 Ref scope

A ref on the line of a `function`, an entry's `handle`, or a `type` is that block's **scope**.
Quote-only refs inside the block search in the scope's target. Scopes nest; the
nearest one wins. A quote-only ref with no scope is an error.

### 8.4 Status

The checker gives every ref a status, shown by the viewer:

| Status | Meaning |
|---|---|
| ✓ ok | target found, quote found |
| ⚠ weak | target found; a name the line mentions (field, model) does not appear in it |
| ✗ broken | target or quote not found |

## 9. Detail levels

The file always contains everything. A **level is a view**: the viewer hides
lines deeper than the chosen level.

### 9.1 The levels

| Level | Name | Question it answers | Reader |
|---|---|---|---|
| L0 | Card | Do I need to look, and where is the danger? | everyone; triage |
| L1 | Flow | What happens, in which order, and what can go wrong? | reviewer's first pass; new team member |
| L2 | Detail | Is the L1 story actually true? | careful review |
| L3 | Full | What exactly does every step do? | debugging; onboarding into this code |

### 9.2 Default level by construct

The writer only adds markers where the default is wrong (see the level guide, §10).

| Construct | Default |
|---|---|
| Version line, review card | L0 |
| `entry({ … })` line, its options, statements directly in `handle` | L1; `trigger`, `access` and `input` cannot be lowered; `guards`, `timeout`, `backoff` are L2 (§6.1) |
| Branches whose block ends in `fail(...)` (error paths) | L2 |
| Other branches and loops | level of the enclosing block |
| Function definitions in `details` | L2 (call sites keep their own level) |
| Type declaration line | L1 |
| Fields of multi-line types, entries of `Events` / `Jobs` | L2 (single-line types are shown whole) |
| `!` and `!?` comments | always shown at L1+ (§9.4) |
| `?` comments, `tested:`, `used by:` | L2 |
| Other comments | level of the statement they are attached to |
| Refs | never shown as text; the viewer shows them as link chips |

### 9.3 Markers

A comment starting with `L1`, `L2` or `L3` sets the level of the statement it is
attached to (§7.2):

```ts
const email = String(data.email).toLowerCase() // L3 @ "Str::lower("
// L1
if (order.status !== "paid") fail(409, "only paid orders can be refunded")
```

**Effective level = max(marker or default, effective level of the parent).**
A child can never be more visible than its parent. A marker that has no effect
(e.g. `L1` inside an L2 block) is a checker warning.

### 9.4 Hide, never lose

When the viewer hides a block, it shows **one placeholder line** in its place:

```
const user = await createUser(req.raw)  ▸ writes User, Address · emits UserRegistered · 1 ! · Δ2
```

The placeholder shows:

- **effect badges** — the prelude categories of the calls inside (`writes`, `effect`, `external`, …) with names,
- **counts** of `!`, `?`, and in diff mode `Δ` changed lines,
- **risk text** — every hidden `!` / `!?` is shown, with its text, under the
  nearest visible ancestor. For a hidden helper function, that is every visible
  call site of the helper.

Any placeholder can be expanded on its own without changing the global level.
A visible call to a helper defined in the file can be expanded **in place**.

### 9.5 Level views

- **L0** shows the card, then the list of all `!` / `!?` (text + jump link), then checker findings (§12).
- **L1–L3** show the body filtered by level, plus §9.4 placeholders.
- **Diff mode** (§11.5) works at every level. Option "show changes": changed lines
  are shown whatever their level, together with their parent lines.

---

## 10. Level guide

> This section is self-contained so it can become a writer skill.

### 10.1 The ladder

For every statement, ask in order. The first "yes" decides.

1. **Can it cause harm?** Security, data loss or corruption, money, privacy,
   double execution, a race, an effect that cannot be undone *and can go wrong* (sent twice, to the wrong
   person, before the data is saved).
   → add a `!` comment (always visible). Then continue with question 2 for the statement itself.
2. **Does it cross a boundary?**
   - *who*: access checks, ownership checks
   - *in*: reading and validating outside input
   - *stored*: writes to DB, files, cache
   - *out*: events, queues, mail, notifications, external calls, the response
   - *decisions that change which boundary is crossed*: a business rule that stops
     a write, a branch that sends a different mail
   → **L1**
3. **Is it needed to check that an L1 line is true?** Field lists, conditions,
   error paths, where a value comes from, helper bodies, tests, callers.
   → **L2**
4. **Does it change behaviour or data at all?** Defaults, normalisation, sorting,
   mapping with logic, minor branches.
   → **L3**
5. **None of these** → leave it out of the file.

### 10.2 Tests for a borderline case

- **Delete test.** If this line were missing from L1, would the reader get a *wrong*
  picture (not just a less detailed one)? Yes → L1.
- **Same-story test.** L1 must be a complete story. L2 adds evidence, not new plot.
  If an L2 line changes the story (an early return that skips the write, a branch
  that sends a different mail), promote it to L1.
- **Stranger test.** Would a developer who has never seen this codebase need the line
  to understand L1? No → L2 or deeper.

### 10.3 Budgets

| Level | Budget | When over budget |
|---|---|---|
| L0 card | ≤ 12 lines, title + summary ≤ 4 lines | Shorten items; move detail to the body. |
| L1 per entry point | ≤ 15 statements | Extract a helper with a domain name (its body becomes L2). Do not lower single lines just to fit. |
| L2 per helper | ≤ 25 statements | Split the helper. |

### 10.4 Worked examples

| Statement | Level | Why |
|---|---|---|
| `authorize("orders.refund", order)` | L1 | who |
| `input: shape<RefundInput>()` (entry option) | L1, cannot be lowered | in |
| `const event = validate<StripeEvent>(req.raw)` (webhook body checked inside the handler) | L1 | in |
| `if (!order) fail(404)` | L2 (default) | plain error path |
| `if (order.status !== "paid") fail(409)` | **L1** (marker) | business rule that stops the write: same-story test |
| `const amount = order.items.reduce((s, i) => s + i.price, 0)` | L2 | evidence for the charge amount |
| `await external("Stripe").refund(order.chargeId, amount)` | L1 + `!` | out, irreversible |
| `await db.update<Order>({ id: order.id }, { status: "refunded" })` | L1 | stored |
| `await cache.forget("order:" + order.id)` | L2 | stored, but only a cache; part of the evidence |
| `const note = String(input.note).trim()` | L3 | changes data, no risk |
| `log("info", "refund started")` | left out | no behaviour |
| `log("info", "refund", { card: input.cardNumber })` | L2 + `!` | personal data in logs |

### 10.5 Anti-patterns

- Putting everything in L1 because "it's important". L1 is the story, not the evidence.
- Hiding a risk by giving it a low level. Risks are `!`, and `!` is always shown.
- Inventing an L1 summary line that is not in the code. Every L1 line needs a ref,
  directly or through its scope.
- Lowering lines to meet the L1 budget instead of extracting a helper.

---

## 11. Diff

### 11.1 Diff-stable writing rules

- One statement per line; a ref sits on the line it explains, so a ref change is a one-line diff.
- **No column alignment** in the body (the viewer aligns comments for display).
  Otherwise one longer line would shift every aligned comment below it.
- Fixed section order (§4) and fixed card field order (§5).
- No line numbers anywhere; moving code does not change the file.
- A formatter normalises whitespace after every write.

### 11.2 Update, don't rewrite

The writer always receives the current file (if one exists) and makes the
**smallest edit** that makes it true again. It does not rephrase lines that are
still true. This is what keeps diffs readable.

### 11.3 Repo mode

The file is committed. When a PR changes the code, the writer updates the file in
the same PR. The PR diff then contains the Flowmark diff, readable with plain
`git diff` or any code host.

### 11.4 Generated mode

No committed file. To diff two commits:

1. Generate the file for the **base** commit (or reuse a cached one).
2. Generate the **head** file **as an edit of the base file**, given the code diff
   between base and head (§11.2).
3. Diff the two files.

Generating head on its own would change the wording of unchanged parts, and the
diff would be noise.

### 11.5 Diff support

- **Plain git** works without tools.
- **Optional textconv** to hide ref-only changes from `git diff`:
  ```
  # .gitattributes
  *.review.ts diff=flowmark
  # git config
  git config diff.flowmark.textconv "flowmark strip-refs"
  ```
- **Viewer diff mode** puts changes that matter first:
  1. changed card fields,
  2. new or removed `!` / `!?`,
  3. changed entry points (`trigger`, `access`, options),
  4. changed types (fields added or removed in inputs, outputs, events),
  5. everything else, filtered by level, with `Δ` badges on placeholders (§9.4).

---

## 12. Checker

The checker runs after every write. **Gates** go back to the writer, which
must fix the file. **Warnings** are shown to the human as checker findings,
separate from the writer's own `!`.

### 12.1 Gates

1. The file parses as TypeScript (tree-sitter).
2. Version line and card are present; required card fields are present, in order.
3. Every ref resolves (§8.2) and has no ✗ status.
4. Every entry is `const <name> = entry({ … })` with `trigger`, `access` and `handle`, each on its own line;
   `input` is `shape<T>()` and only on entries that receive input from outside.
5. No ambiguous comment attachment; no quote-only ref without a scope.

### 12.2 Card ↔ body

| Card field | Compared with |
|---|---|
| `access` | `access` of every entry |
| `input` | the entries' `input: shape<T>()` and `validate<T>` type arguments; `raw request` required if a `Raw` field of an entry's input (`raw`, `params`, `headers`, `meta`, `args`, `options`) is used outside `validate` |
| `writes` | type arguments of `db.insert/update/upsert/delete`, plus `files.write/delete`, `cache.set/forget` |
| `effects` | `emit`, `queue`, `mail`, `notify`, `external` calls by name |
| `output` | `respond` status codes and the return types of `handle` |

A mismatch is a gate error.

### 12.3 Warnings

- **Type check** with `tsc`: each file in its own program with `prelude.d.ts`,
  options `strict`, `noUnusedLocals`, `lib: ["es2022"]`, `types: []`.
  Example of a real finding: *"Property 'role' does not exist on type 'SignupInput'"*:
  the handler uses a field the validated input does not have.
- **Coverage.** Calls inside the cited symbols that no ref covers are listed as
  *possibly left out*. This is the main guard against the writer missing something.
- **Lints** (possible because writes and effects go through the prelude):
  - an entry declares `input`, but `handle` reads `req.raw` (`W060`): the validated input is bypassed
  - unvalidated `Raw` data reaches `db.insert/update/upsert` without a `!`
  - `db.list` without `limit` (unbounded read)
  - `db.*` or `external()` inside a loop (one query or call per item)
  - `mail`, `notify`, `external`, `queue` inside `db.transaction` without `afterCommit` (effect before commit)
  - `retries > 0` without `idempotent: true` (may run twice)
  - level marker without effect (§9.3); a level marker that hides a write or effect (`W053`); L1 or L2 budget exceeded (§10.3)

### 12.4 Output: index

The checker writes an index next to the file (or keeps it in memory in generated
mode). Viewers read the Flowmark file **and** the index; they never parse the
original code.

```json
{
  "file": "docs/review/register-user.review.ts",
  "commit": "a1b2c3d",
  "refs": [
    { "line": 49, "ref": "StoreUserRequest.rules", "status": "ok",
      "target": { "path": "app/Http/Requests/StoreUserRequest.php", "start": 14, "end": 24 } }
  ],
  "lines": [ { "line": 49, "level": 1, "categories": ["input"] } ],
  "risks": [ { "line": 52, "kind": "!", "text": "passes req.raw instead of req.input: …" } ],
  "findings": [ { "kind": "W060", "line": 53, "message": "`registerUser` declares `input`, but `handle` reads `req.raw`, which is not validated" } ]
}
```

---

## 13. Writing rules (for the AI writer)

1. Read the code first; write only what refs can support.
2. Follow the layout (§4), the card rules (§5) and the level guide (§10).
3. Every statement that touches access, input, data, effects or output has a ref,
   directly or through its scope.
4. Use `entry()` for every entry point and the prelude for every boundary; use domain names for entries and helpers.
   Input the framework validates up front is `input: shape<T>()` on the entry, not a `validate()` line.
5. Show data with types; show exposed fields with `pick` / `Pick`.
6. Mark risks with `!`, possible risks with `!?`, and doubts with `?`. Never guess silently.
7. When a file exists, edit it minimally (§11.2).
8. Fix every gate error the checker reports, then stop.

---

## 14. Example

Full file: [`examples/register-user.review.ts`](examples/register-user.review.ts).
It type-checks against the prelude, and `flowmark check` reports exactly one finding, which is
the real bug: `registerUser` declares `input`, but `handle` passes `req.raw` on (`W060`).

### 14.1 Source (excerpt)

```ts
// --- flow ---

// tested: @ tests/Feature/SignupTest.php "test_user_can_register"
// ! untested: invalid input, role injection
const registerUser = entry({
  trigger: "POST /api/users", // @ routes/api.php "Route::post('/users'"
  access: "public",
  input: shape<SignupInput>(), // @ StoreUserRequest.rules
  rateLimit: "10/min", // @ routes/api.php "throttle:10,1"
  async handle(req): Promise<UserResponse> { // @ UserController.store
    // ! passes req.raw instead of req.input: role and address.zip reach the DB unvalidated
    const user = await createUser(req.raw) // @ "$request->all()"
    return respond(201, pick(user, "id", "name", "email")) // @ UserResource.toArray
  },
})

// --- details ---

// used by: admin user creation @ AdminUserController.store "createUser("
async function createUser(data: Raw) { // @ UserService.create
  return db.transaction(async () => { // @ "DB::transaction("
    const email = String(data.email).toLowerCase() // L3 @ "Str::lower("
    const user = await db.insert<User>({ ...data, email, password: hash(data.password) }) // @ "User::create("
    await db.insert<Address>({ ...data.address, userId: user.id }) // @ "->address()->create("
    // ! the listener is queued before commit: the mail can go out for a rolled-back user
    emit("UserRegistered", { user }) // @ "event(new UserRegistered"
    return user
  })
}

// --- async ---

const sendWelcomeMail = entry({
  trigger: "event UserRegistered", // @ EventServiceProvider "UserRegistered::class"
  access: "system",
  queued: true, // @ SendWelcomeMail "implements ShouldQueue"
  retries: 2, // 3 attempts in total @ SendWelcomeMail "$tries = 3"
  handle({ user }) { // @ SendWelcomeMail.handle
    mail(user.email, "WelcomeMail") // ! irreversible: a retry after a timeout can send it twice
  },
})
```

### 14.2 Rendered at L0

Output of `flowmark view examples/register-user.review.ts --level 0`.

```
Register user
Public signup. Creates the user and their address in one transaction,
then sends a welcome mail through the queue.

access:   public (registerUser), system (sendWelcomeMail)
input:    SignupInput, raw request !
writes:   User, Address (1 transaction)
effects:  event UserRegistered → mail WelcomeMail (queued)
output:   201 UserResponse
shared:   createUser, also used by admin user creation
tests:    happy path only

risks
  ! mass-assignable: a client that reaches User.create can set role=admin       → type User
  ! untested: invalid input, role injection                                     → registerUser
  ! passes req.raw instead of req.input: role and address.zip reach the DB un…  → registerUser
  ! the listener is queued before commit: the mail can go out for a rolled-ba…  → createUser
  ! irreversible: a retry after a timeout can send it twice                     → sendWelcomeMail

checker
  ⚠ W060 `registerUser` declares `input`, but `handle` reads `req.raw`, which…  → line 53
```

### 14.3 Rendered at L1

Output of `flowmark view … --level 1`. Refs are left out (a viewer shows them as links).

```ts
// --- data ---

type SignupInput = { … 4 fields }

type User = { … 5 fields }  ▸ 1 ! · 1 ?
  ! mass-assignable: a client that reaches User.create can set role=admin

type Address = { userId: Id; street: string; zip: string }

type UserResponse = Pick<User, "id" | "name" | "email">

interface Events { … 1 event }

// --- flow ---

// ! untested: invalid input, role injection
const registerUser = entry({
  trigger: "POST /api/users",
  access: "public",
  input: shape<SignupInput>(),
  rateLimit: "10/min",
  async handle(req): Promise<UserResponse> {
    // ! passes req.raw instead of req.input: role and address.zip reach the DB unvalidated
    const user = await createUser(req.raw)  ▸ writes User, Address · transaction · emits UserRegistered · 1 !
      ! the listener is queued before commit: the mail can go out for a rolled-back user
    return respond(201, pick(user, "id", "name", "email"))
  },
})

// --- async ---

const sendWelcomeMail = entry({
  trigger: "event UserRegistered",
  access: "system",
  queued: true,
  retries: 2,  // 3 attempts in total
  handle({ user }) {
    mail(user.email, "WelcomeMail")  // ! irreversible: a retry after a timeout can send it twice
  },
})
```

### 14.4 L2 and L3

- **L2** adds: type fields, the `?` on `role`, `tested:` and `used by:`, and the
  `details` section (the body of `createUser`, also expandable at the call site).
- **L3** adds: `const email = String(data.email).toLowerCase()`.

### 14.5 A PR, as a diff

```diff
  * access:   public
- * writes:   User, Address (1 transaction)
+ * writes:   User, Address, Subscription (1 transaction)
- * effects:  event UserRegistered → mail WelcomeMail (queued)
+ * effects:  event UserRegistered → mail WelcomeMail (queued), external Stripe.createCustomer !
  ...
     await db.insert<Address>({ ...data.address, userId: user.id }) // @ "->address()->create("
+    // ! external call inside the transaction: a rollback does not undo the Stripe customer
+    const customer = await external("Stripe").createCustomer(email) // @ BillingService.register "Customer::create("
+    await db.insert<Subscription>({ userId: user.id, customerId: customer.id }) // @ BillingService.register "Subscription::create("
```

A change to who may call an entry is always a one-line change in the card and a
one-line change in the entry:

```diff
- * access:   public (registerUser), system (sendWelcomeMail)
+ * access:   user (registerUser), system (sendWelcomeMail)
  ...
  const registerUser = entry({
    trigger: "POST /api/users", // @ routes/api.php "Route::post('/users'"
-   access: "public",
+   access: "user", // @ routes/api.php "middleware('auth')"
```

---

## 15. Open questions and roadmap

1. **Name.** "Flowmark" is a working name.
2. **Writer skill.** Done: `skills/flowmark-review/` (§10 is shipped as its level guide).
3. **Checker CLI.** `flowmark check | view | parse | model | html` exist (see README). Still to do: resolving
   refs in the reviewed code (§8.2), card ↔ body (§12.2), the type check (§12.3), `strip-refs` (§11.5).
4. **Viewer.** Done as `viewer/viewer.js`: levels, placeholders, bubbled risks, ref chips, diff mode. It reads
   only the view model (`flowmark model`), never the original code.
5. **Browser and editor.** The browser app (`web/`, parser in WebAssembly) is done. Still to do: a VS Code
   extension (link provider for refs, folding by level, diff view).
6. **Secrets.** Track `Secret` values through the file and warn when one reaches
   `respond`, `log`, `mail` or `external`.
7. **Several capabilities.** An overview file that lists all capabilities and their
   cards, for a "what does this PR touch" page.
