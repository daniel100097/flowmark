---
name: flowmark-review
description: Write or update a Flowmark review file (*.review.ts), which is AI-written pseudo-TypeScript that explains a code path or a pull request to a human reviewer. Every claim cites the real code, risks are marked, and the file reads at four detail levels. Use when asked to document how a request, RPC, job, event or command flows through a codebase, to review a PR or diff as a Flowmark file, or to update files under docs/review/.
---

# Flowmark review

Write a `*.review.ts` file that a reviewer reads instead of the code. Next to this file:
- `examples/register-user.review.ts`: copy its style.
- `prelude.d.ts`: the only functions you may call.
- `reference/levels.md`: the detail-level guide, for when you're unsure.

## Steps

1. **Scope.** Write one file per capability (the entry points that belong together) at
   `docs/review/<capability>.review.ts`. If the file exists, edit it minimally; never reword lines that are
   still true.
2. **Read the code** from where it starts: registration, middleware, validation, handler, helpers, every
   write and side effect, then the listeners and jobs it triggers.
3. **Write the file**, then repeat until clean:
   - `flowmark check <file>` (no errors; fix warnings unless one is a real finding you mark `!`)
   - `npx -y -p typescript@5 tsc --noEmit --strict --noUnusedLocals --lib es2022 --target es2022 <this-dir>/prelude.d.ts <file>`
   - grep every ref: the symbol exists, and the quote is inside it
   - `flowmark view <file> --level 1` tells the whole story

   No `flowmark`? Run `curl -fsSL https://raw.githubusercontent.com/daniel100097/flowmark/main/scripts/install.sh | sh`.
4. **Report:** what you documented and what you left out, the risks, the check results, refs you could not
   verify, and anything the language couldn't express.

## Rules

- **Card:** `access`, `input`, `writes`, `effects` and `output` are required (write `none` when empty); it
  must match the body.
- **Entries:** `const <domainName> = entry({ trigger, access, input?, ...options, handle })`, one option per line.
  - Triggers: `"POST /path"`, `"event X"`, `"job X"`, `"message topic"`, `"schedule ..."`, `"cli cmd"`,
    `"rpc S.M"`, `"query|mutation x"`, `"ws x"`.
  - `input: shape<T>()` when the framework validates before the handler; then use `req.input`. `req.raw`
    is not validated.
  - Events need `queued`.
- **Body:** pseudocode, one idea per line. Use only prelude calls (`db.*<T>`, `emit`, `queue`, `mail`,
  `external("Svc").op`, `authorize`, `fail`, `respond`, …) and what you define. Pure helpers may be
  `declare function`. Leave out logging, dependency injection and glue code.
- **Refs:** `// @ Symbol`, `// @ path/file.ext`, `// @ Symbol "exact quote"`, or `// @ "quote"` (searched in the
  ref of the enclosing function, handle or type). No line numbers. Every statement that touches access,
  input, data, effects or output needs one.
- **Risks:** `// ! <what goes wrong>` for bugs, security, money, data loss, privacy, double execution or races;
  `// !? <unconfirmed risk>`; `// ? <fact you're unsure of>`. That an effect can't be undone is not a risk by
  itself. A long risk continues on plain `//` lines.
- **Levels:** the defaults are usually right, so only mark exceptions with `// L2` or `// L3`.
  - L1 is the story: every boundary (who, what comes in, what is stored, what goes out).
  - L2 is the evidence; L3 is trivia.
  - Never lower a write or an effect.

## Pull requests

Write the file at the merge-base as `base.review.ts`, then write the head version as a minimal edit of it
(`head.review.ts`). Run `flowmark html base.review.ts head.review.ts -o pr.html`.

If something can't be expressed, use the closest construct, mark it `?`, and report it.
