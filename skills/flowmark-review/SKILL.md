---
name: flowmark-review
description: Write or update a Flowmark review file (*.review.ts), which is AI-written pseudo-TypeScript that explains a code path or a pull request to a human reviewer. Every claim cites the real code, risks are marked, and the file reads at four detail levels. Use when asked to document how a request, RPC, job, event or command flows through a codebase, to review or summarize a PR or diff as a Flowmark file, or to update files under docs/review/. Not for ordinary code comments, READMEs or API docs.
---

# Flowmark review

You write a `*.review.ts` file that a human reads **instead of** the code. It says what the code does,
where each part lives, what needs attention, and (in a PR) what changed.

Files next to this one:
- `prelude.d.ts`: the only vocabulary you may call, plus what the file defines itself.
- `examples/register-user.review.ts`: a small example with five risks.
- `examples/t3code-update-server-settings.review.ts`: a real one (a TypeScript RPC with secret handling).
- `reference/levels.md`: the full detail-level guide.

The full spec is at https://github.com/daniel100097/flowmark/blob/main/SPEC.md. Read it when this file
does not settle a question.

## Tools

```sh
command -v flowmark || cargo install --locked --git https://github.com/daniel100097/flowmark flowmark_cli
flowmark check <file>              # structure, entries, comments, refs syntax, levels
flowmark view  <file> --level 1    # what a reader sees at L1 (also 0, 2, 3)
flowmark html  <file> -o out.html  # interactive page; with base and head: a PR diff
npx -y -p typescript@5 tsc --noEmit --strict --noUnusedLocals --lib es2022 --target es2022 \
  <this-skill-dir>/prelude.d.ts <file>          # type check against the prelude
```

If you cannot install the CLI (no Rust ≥ 1.99) or run tsc (no Node), do those checks by hand and
say so in your report.

## Workflow

1. **Scope.** Write one file per capability ("Register user", "Refund order"). A capability is the entry
   points that belong together: a route, its listener, its job. For a PR, list the capabilities the diff
   touches. If there are many, do the most important and list the rest in your report.
2. **Existing file?** Look in `docs/review/` (or wherever the repo keeps them). If one exists, **edit it
   minimally**: change only what the code change made untrue, and add what is new. Never rephrase a line
   that is still true, because the diff of this file is what reviewers read.
3. **Read the code** from where execution starts: route or listener registration, middleware and guards,
   validation, the handler, its helpers, every write and side effect, and what runs later (listeners,
   jobs, scheduled work). Read beyond the diff.
4. **Write** the file using the layout and rules below.
5. **Check** until clean (see "Self-check").
6. **Report** (see the last section).

New files go to `docs/review/<capability>.review.ts` unless the repo already has another place.

## Layout (fixed order)

```ts
// flowmark 0.1
/**
 * Refund order
 * <1–3 lines: what this does and why it exists>
 *
 * access:   user (refundOrder), system (sendRefundMail)
 * input:    RefundInput
 * reads:    Order
 * writes:   Order, Refund (1 transaction)
 * effects:  external Stripe.refund !, event OrderRefunded → mail RefundMail (queued)
 * output:   200 RefundResponse
 * config:   flag refunds-v2
 * shared:   loadOrder, also used by order view
 * tests:    happy path, already refunded
 */

// --- data ---      types: inputs, models, outputs, interface Events, interface Jobs
// --- flow ---      entries started from outside or by time
// --- details ---   helper functions
// --- async ---     entries started by this app: event listeners, jobs
```

- **Card fields** stay in this order. `access`, `input`, `writes`, `effects` and `output` are required;
  write `none` when one is empty. `reads`, `config`, `shared` and `tests` are optional.
- **Card items** are separated by `,` or `→`. A `(note)` and a trailing `!` (needs attention) are allowed.
- **The card must match the body:**
  - `access`: one item per entry
  - `input`: the `input:` shapes, plus `raw request` if unvalidated input is used
  - `writes`: every `db.*<T>` write, plus `files` and `cache` writes
  - `effects`: every `emit`, `queue`, `mail`, `notify` and `external` call
  - `output`: status and type

## Entries

```ts
const refundOrder = entry({
  trigger: "POST /orders/:id/refund", // @ routes/api.php "Route::post('/orders/{order}/refund'"
  access: { ability: "orders.refund" }, // @ OrderPolicy.refund
  input: shape<RefundInput>(), // @ RefundRequest.rules
  rateLimit: "5/min", // @ routes/api.php "throttle:5,1"
  async handle(req): Promise<RefundResponse> { // @ RefundController.store
    const order = await db.findOrFail<Order>({ id: req.params.id }) // @ "Order::findOrFail("
    // L1
    if (order.status !== "paid") fail(409, "only paid orders can be refunded") // @ "abort_unless($order->isPaid(), 409"
    …
  },
})
```

- **Triggers:**
  - HTTP: `"GET|POST|PUT|PATCH|DELETE /path"`
  - from this app: `"event Name"`, `"job Name"`
  - other sources: `"message topic"`, `"schedule <when>"`, `"cli command"`
  - APIs: `"query|mutation|subscription name"`, `"rpc Service.Method"`, `"ws name"`
- **`access` is always written:** `"public"`, `"user"`, `"internal"`, `"system"`, `{ role }`, `{ ability }` or
  `{ signature }` (webhooks).
- **`input: shape<T>()`** when the framework validates the input before the handler runs (form requests,
  DTO pipes, schema-decoded payloads). The handler then reads `req.input`; `req.raw` is unvalidated.
  Use `validate<T>()` only for validation that really is a step inside the handler.
- **One option per line**, in this order: trigger, access, input, queued, rateLimit, retries, idempotent,
  unique, overlap, single, guards, timeout, backoff, handle.
- **Events need `queued`.** `input` is only for entries that receive data from outside (not events, jobs
  or schedules).
- **Name the const** for the domain (`refundOrder`). HTTP, GraphQL, RPC and WebSocket handlers declare
  their return type.

## Body

- **Pseudocode, not translation.** One line is one idea. Leave out:
  - logging, unless it holds personal data or is an audit trail
  - dependency-injection wiring, getters and setters, framework glue, plain conversions, metrics
- **Only the prelude, plus what the file defines.**
  - Writes and effects always go through the prelude: `db.insert<User>(…)`, `db.transaction`, `db.afterCommit`,
    `emit`, `queue`, `mail`, `notify`, `external("Stripe").refund(…)`, `cache.*`, `files.*`, `lock`.
  - Control flow and output use: `authorize`, `fail`, `assert`, `respond`, `redirect`, `config`, `flag`.
- **Data as TypeScript types.** Show exposed fields with `pick` / `Pick`, and unvalidated data as `Raw`.
- **Helpers** are named for the domain, with the real names in refs. A pure helper whose body doesn't
  matter may be declared:
  `declare function redact(s: Settings): Settings // @ redactForClient`.
  Never `declare` a helper that writes or has effects; write those out.
- **Framework terms only in comments** (`$fillable`, `FormRequest`, `@Transactional`).
- **No imports or exports.** One statement per line. No column alignment (except card field names).

## Refs: every claim cites code

A ref can sit anywhere in a comment: `@ Target "quote"`.

| Form | Example |
|---|---|
| symbol | `@ UserService.create` (also `UserService::create`) |
| path | `@ app/Models/User.php` |
| path + symbol | `@ app/Services/UserService.php#create` |
| symbol + quote | `@ UserController.store "$request->all()"` |
| quote only | `@ "User::create("`: searched in the **enclosing scope**, i.e. the ref on the first line of the enclosing function, `handle` or type |

- **No line numbers, ever.** Copy quotes verbatim from the code (whitespace may differ). Write a `"`
  inside a quote as `\"`.
- **Every statement that touches** access, input, data, effects or output has a ref, directly or through
  its scope.
- **Verify every ref yourself** (grep or ripgrep):
  - the symbol exists; if the name is ambiguous, add the path
  - the quote occurs inside that symbol, or for quote-only refs inside the scope's symbol
  - for a PR, check base refs against the base commit and head refs against head

  A ref you cannot verify stays in the file and goes in your report.

## Comments

| Comment | Use for |
|---|---|
| `// ! text` | **risk**: bug, security, data loss, money, privacy, double execution, irreversible effect, race. Name the concrete consequence ("a client can send role=admin"), not a vague worry. |
| `// !? text` | a possible risk you could not confirm |
| `// ? text` | a fact you are unsure of; the reader should check it |
| `// tested: @ path "test name"` | the test that covers the next statement |
| `// used by: <who> @ Caller.method "call("` | other callers of the next function |
| `// L2`, `// L3` | detail-level marker for the statement |
| `// text` | explanation |

Put a comment **above** the statement it explains, or at the end of its line. A comment with a blank line
below it attaches to nothing (warning W040).

## Detail levels

The file holds everything; a level only decides what a reader sees.
- **L0:** the card and the risks
- **L1:** the story: what happens, in which order, and what can go wrong
- **L2:** the evidence for the L1 story
- **L3:** everything

**Defaults (only mark where they are wrong):**

| Level | What |
|---|---|
| L1 | entry lines and the statements directly in `handle` |
| L2 | helper bodies |
| L2 | fields of multi-line types (single-line types are shown whole) |
| L2 | `if (…) fail(…)` error branches |
| L2 | `?`, `tested:` and `used by:` comments |
| always shown | `!` and `!?` |

**For each statement, ask in order; the first yes wins:**

1. Can it cause harm? Add a `!` comment, then keep going.
2. Does it cross a boundary?
   - who (access)
   - in (input)
   - stored (writes)
   - out (effects, output)
   - a decision that changes which boundary is crossed

   If yes: **L1**.
3. Is it needed to check that an L1 line is true? **L2**
4. Does it change behaviour or data at all? **L3**
5. Otherwise, leave it out.

**Hard rules:**
- Never put `// L2` or `// L3` on a statement that writes or has an effect, directly or through a helper.
  That would hide the main action from L1.
- Never lower `trigger`, `access` or `input`.
- Budgets:
  - card: 12 lines at most
  - L1: 15 statements per entry at most; when over, extract a helper with a domain name rather than
    lowering single lines
  - helpers: 25 statements at most

Worked examples, the delete / same-story / stranger tests, and anti-patterns: `reference/levels.md`.

## Pull requests

1. Check out the PR's merge-base. Write the file for that state (or bring the existing one up to it):
   `base.review.ts`.
2. Check out the head. Produce `head.review.ts` as a **minimal edit of base**: same wording wherever the code
   did not change.
3. Run `flowmark html base.review.ts head.review.ts -o pr.html`. That gives one page with Changes, Before
   and After, and a summary of changed card fields, new risks, and changed entries and types.

If the repo keeps review files under `docs/review/`, just update the file in the PR; its git diff is the
review.

## Self-check (repeat until clean)

1. `flowmark check <file>` reports no errors.
   - Fix every warning, or keep it on purpose with a `!` comment that explains it.
   - Example: W060 when the code really passes the raw request on. That is a finding, not a mistake in
     your file.
2. The type check against `prelude.d.ts` passes.
3. Every ref is verified (see Refs).
4. `flowmark view <file> --level 1` tells the whole story on its own.
5. `--level 0` shows a card that matches the body.
6. Every `!` names a concrete consequence and has a ref to the code that causes it.

## When the language cannot express something

Use the closest construct, add a `?` comment saying what is approximated, and list it as a spec gap in
your report. Known gaps:
- client-side UI actions
- streams pushed to clients
- access that depends on the input
- typed RPC errors
- build-time flags

Do not invent prelude functions.

## Report

When done, tell the user:
- the capability you documented, and what you left out
- the risks, one line each
- the results of `flowmark check` and the type check
- spec gaps
- refs you could not verify
- how to view it: `flowmark html <file> -o <file>.html`, or for a PR,
  `flowmark html base.review.ts head.review.ts -o pr.html`
