<!-- Generated from SPEC.md §10 by scripts/sync-skill.sh. Do not edit here. -->

# Level guide

## The ladder

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

## Tests for a borderline case

- **Delete test.** If this line were missing from L1, would the reader get a *wrong*
  picture (not just a less detailed one)? Yes → L1.
- **Same-story test.** L1 must be a complete story. L2 adds evidence, not new plot.
  If an L2 line changes the story (an early return that skips the write, a branch
  that sends a different mail), promote it to L1.
- **Stranger test.** Would a developer who has never seen this codebase need the line
  to understand L1? No → L2 or deeper.

## Budgets

| Level | Budget | When over budget |
|---|---|---|
| L0 card | ≤ 12 lines, title + summary ≤ 4 lines | Shorten items; move detail to the body. |
| L1 per entry point | ≤ 15 statements | Extract a helper with a domain name (its body becomes L2). Do not lower single lines just to fit. |
| L2 per helper | ≤ 25 statements | Split the helper. |

## Worked examples

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

## Anti-patterns

- Putting everything in L1 because "it's important". L1 is the story, not the evidence.
- Hiding a risk by giving it a low level. Risks are `!`, and `!` is always shown.
- Inventing an L1 summary line that is not in the code. Every L1 line needs a ref,
  directly or through its scope.
- Lowering lines to meet the L1 budget instead of extracting a helper.
