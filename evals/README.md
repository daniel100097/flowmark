# Skill evals

These measure how well an agent writes Flowmark reviews with the `flowmark-review` skill. Each case is
a small codebase with **planted bugs** and a known ground truth.

| Case | Code | Planted bugs |
|---|---|---|
| `php-place-order` | Laravel: `POST /orders`, a service with a DB transaction, a Stripe charge, a queued mail listener | the client sets the prices; the Stripe charge runs inside the transaction; the mail can be sent twice or before commit |
| `ts-password-reset` | Express + zod + Prisma: forgot and reset password | the token comes from `Math.random`; tokens never expire; tokens can be reused |
| `go-stripe-webhook` | Go `net/http` + pgx: `invoice.paid` credits, subscription cancel + SendGrid mail | no idempotency (double credit); a credit error is only logged and still answers 200; an email address is logged |
| `pr-password-reset` | a git repo whose last commit is a PR on `ts-password-reset` | the PR fixes the token and the expiry, but puts the new password in a mail |
| `update-place-order` | the Laravel app **with an existing review**; the last commit takes prices from the DB and adds an `Idempotency-Key` | the old price risk must go; the key is not scoped per user (another user's order comes back); the untouched listener must stay byte-identical |
| `py-plan-upgrade` | FastAPI + SQLAlchemy + Celery, 9 files for the capability, plus unrelated modules and a dead `legacy/` module with a second `charge` (a trap for ambiguous refs) | hidden plans can be bought; `password_hash` sent to HubSpot; the hourly trial job downgrades paid upgrades (only visible by reading the scheduled job) |
| `rails-team-invite` | Rails invitations, meant as **clean code** to measure false alarms | none planted; the agents found 3 real bugs anyway, now listed as known issues; any other `!` counts as a false alarm |

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
| 5 | reads (when the truth lists them) |
| 5 | focus: no refs into dead code (when the case has some) |
| 20 | precision: no `!` beyond planted and known issues (clean-code case) |
| 5 | the untouched entry stays byte-identical (update case) |

A level marker that hides a write or effect costs 5 points. A symbol defined in several files, cited
without a path, counts as a failed ref (SPEC §8.2). The grader also reports the length of each
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

## Results 2026-11-08, round 2: harder cases, Opus and Haiku

Setup: long and short skill, each with Claude Opus 5.5 and with Claude Haiku 5.5, on the three new cases
(12 runs). Then three reruns of the weakest cell after fixing what it exposed.

| Case | Opus long | Opus short | Haiku long | Haiku short |
|---|---|---|---|---|
| update-place-order | 100 | 100 | 100 | 100 |
| py-plan-upgrade | 100 | 100 | 100 | 100 |
| rails-team-invite | 92 | **100** | 92 | **78** → 92, 92, **100** (reruns) |
| tokens (3 cases) | 261k | 235k | 349k | 320k |

What this shows:
- **Updating works.** Every agent edited the existing file minimally (5–6% of lines), removed the fixed
  price risk, found the new idempotency-key leak, and left the untouched listener byte-identical.
- **Deep bugs are found, traps are avoided.** All 4 found the three Python bugs, including the trial
  downgrade that only shows up when reading the scheduled job, and none cited the dead `legacy/` code.
- **The agents beat the test author.** In the "clean" Rails code they found 3 real bugs I had not planted:
  expired invitations block re-inviting forever, the rate limit is bypassed with a `.json` suffix, and a
  missing email gives a 500.
- **Haiku with the short skill was the weak spot**, and the runs showed why. All causes are fixed:
  - **The language made it flag a non-issue.** The prelude said a `Secret` must "never reach mail", so
    the invitation token in the invitation mail looked like a risk. It is now allowed when that is the
    secret's purpose.
  - **A `// L2` marker hid the main `db.insert` from L1.** The new check **W053** catches markers that
    hide a write or effect; the skill's check loop runs it. The t3 agent made the same mistake earlier.
  - **Generic notes like "the mail can't be recalled".** The spec and skill now say that irreversible
    is not a risk by itself: name what goes wrong.
  - **One risk written over two `// !` lines** showed up as two risks. A long risk now continues on
    plain `//` lines and the parser joins them.

  After these fixes, the rerun scored 100 with no false alarms.
- **The short skill stays.** It matches the long one on Opus and, after the fixes, on Haiku, using 8–10%
  fewer tokens.
- Each cell is one run (the reruns show some variance: 92, 92, 100).
