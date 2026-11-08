# Skill evals

These measure how well an agent writes Flowmark reviews with the `flowmark-review` skill. Each case is
a small codebase with **planted bugs** and a known ground truth.

| Case | Code | Planted bugs |
|---|---|---|
| `php-place-order` | Laravel: `POST /orders`, a service with a DB transaction, a Stripe charge, a queued mail listener | the client sets the prices; the Stripe charge runs inside the transaction; the mail can be sent twice or before commit |
| `ts-password-reset` | Express + zod + Prisma: forgot and reset password | the token comes from `Math.random`; tokens never expire; tokens can be reused |
| `go-stripe-webhook` | Go `net/http` + pgx: `invoice.paid` credits, subscription cancel + SendGrid mail | no idempotency (double credit); a credit error is only logged and still answers 200; an email address is logged |
| `pr-password-reset` | a git repo whose last commit is a PR on `ts-password-reset` | the PR fixes the token and the expiry, but puts the new password in a mail |

`grade.mjs` scores a run from 0 to 100:

| Points | Check |
|---|---|
| 15 | valid: `flowmark check` errors and type check |
| 10 + 5 | entries: trigger and access |
| 5 | input shapes |
| 10 | writes |
| 10 | effects |
| 30 | planted bugs found as `!` risks |
| 15 | refs that exist in the code (catches invented symbols and quotes) |
| 25 (PR only) | risks added and removed, card changes, minimal base → head diff |

A level marker that hides a write or effect costs 5 points. The grader also reports the length of each
file.

## Running

```sh
cargo build --release
node evals/setup.mjs <case> <run-dir>     # fresh working copy (the PR case is a git repo)
# Give a fresh agent this prompt (an agent that has not seen this repo):
#   Read <skill-dir>/SKILL.md and follow it. Request: "<case.json prompt>".
#   Work only in <run-dir> and <skill-dir>.
FLOWMARK=$PWD/target/release/flowmark node evals/grade.mjs <case> <run-dir>
```

`variants/long/SKILL.md` is the first, long version of the skill, kept as a baseline for A/B runs.

## Results 2026-11-08: long vs short skill

Setup: one Claude Opus 5.5 subagent per case and variant, with the same prompt and the same skill files
except `SKILL.md`. The long version has 268 lines (2,027 words); the short one, now the skill, has
58 lines (555 words).

| Case | Skill | Score | Planted bugs | Risks written | Refs verified | Errors / tsc | Lines | Tokens | Time |
|---|---|---|---|---|---|---|---|---|---|
| php-place-order | long | **100** | 3/3 | 5 | 47/47 | 0/0 | 102 | 83k | 370s |
| php-place-order | short | **100** | 3/3 | 6 | 38/38 | 0/0 | 111 | 66k | 228s |
| ts-password-reset | long | **100** | 3/3 | 8 | 35/35 | 0/0 | 77 | 81k | 348s |
| ts-password-reset | short | **100** | 3/3 | 7 | 38/38 | 0/0 | 85 | 65k | 201s |
| go-stripe-webhook | long | **100** | 3/3 | 12 | 43/43 | 0/0 | 124 | 82k | 355s |
| go-stripe-webhook | short | **100** | 3/3 | 11 | 42/42 | 0/0 | 130 | 82k | 354s |
| pr-password-reset | long | **100** | 2/2 | 7 | 75/75 | 0/0 | 86/86 | 90k | 368s |
| pr-password-reset | short | **100** | 2/2 | 4 | 78/78 | 0/0 | 84/92 | 70k | 278s |
| **total** | long | | 11/11 | | 200/200 | | | 336k | 24.0 min |
| **total** | short | | 11/11 | | 196/196 | | | 283k | 17.7 min |

What this shows:
- **Same quality, less cost.** Both versions found every planted bug, wrote valid files, and invented no
  refs: 396 of 396 verified. The short version used 16% fewer tokens and 26% less time.
- **The PR workflow works.** Both wrote head as a minimal edit of base (10–11% of base lines changed). The
  diff summary showed the two fixed risks as removed and the new one (password in the mail) as added.
- **Beyond the planted bugs**, both found real extra issues: no idempotency on order placement, account
  enumeration by response timing, ignored SendGrid/Postmark answers, a missing migration for `expiresAt`.
- **Limits.** The cases do not separate the two versions (both reach 100), so they cannot detect a small
  regression. Harder cases would be needed for that, e.g. updating an existing file, a bigger codebase, or
  a capability spread over many files. Each cell is one run.

Fixed after these runs, from the agents' reports:
- The spec example wrote `retries: 3` for Laravel's `$tries = 3` (3 attempts); it is now `retries: 2`.
- The prelude no longer says invalid input always fails with 422.

Open spec gaps the agents reported:
- `Where<T>` cannot say "greater than".
- `UPDATE … RETURNING` has no form.
- Passing a `Secret` to `mail` is not flagged by any check.
