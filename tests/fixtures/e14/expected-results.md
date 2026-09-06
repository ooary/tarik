# E14 data-trust fixture expected results

Use `data-trust.sql` only in a disposable local project. Run the setup section first, create the checks below, inspect the failures, then run the explicit repair section.

## Initial profile facts

Profile `main.orders` in exact distinct mode.

| Fact                      |                                Expected value | Provenance |
| ------------------------- | --------------------------------------------: | ---------- |
| Row count                 |                                             5 | Exact      |
| `customer_id` NULL count  |                                             1 | Exact      |
| `order_id` distinct count |                                             4 | Exact      |
| `amount` minimum          |                                         -7.00 | Exact      |
| `amount` maximum          |                                       1500.00 | Exact      |
| `ordered_at` minimum      |                           2026-08-10 08:00:00 | Exact      |
| `status` common values    | `paid` count 2; the other values count 1 each | Exact      |

Representative values are Sampled and their order is not an assertion.

## Saved check definitions and initial outcomes

| Check                       | Type            | Target/options                                                            | Initial outcome | Exact failure count |
| --------------------------- | --------------- | ------------------------------------------------------------------------- | --------------- | ------------------: |
| Customer ID required        | not null        | `orders.customer_id`                                                      | Failed          |                   1 |
| Order ID unique             | unique          | `orders.order_id`; ignore NULL                                            | Failed          |                   2 |
| Amount in operating range   | range           | inclusive 0 through 1000; ignore NULL                                     | Failed          |                   2 |
| Known order status          | accepted values | `paid`, `pending`, `returned`; NULL fails                                 | Failed          |                   1 |
| Customer exists             | relationship    | `orders.customer_id` to `customers.customer_id`; ignore NULL              | Failed          |                   1 |
| Recent fixed-boundary order | custom SQL      | `SELECT * FROM orders WHERE ordered_at < TIMESTAMP '2026-09-01 00:00:00'` | Failed          |                   1 |
| Orders available            | not empty       | `orders`                                                                  | Passed          |                   0 |

A real freshness check compares with current time, so its result changes as time passes. The fixed-boundary custom check exists only to keep automated fixture expectations deterministic.

## Evidence and preview rules

- Generated count and failure SQL are visible before execution.
- Copy SQL and Open SQL do not execute.
- Every initial assertion result is exact.
- Opening a failed historical run after restart resolves its immutable revision.
- Failure examples are labeled **Current-data preview using revision N**. They are not retained historical rows.
- The preview uses existing 500-row result pages and must be explicitly released when closed.
- Updating a definition does not alter older aggregate runs or silently retarget historical reruns.

## Expected outcomes after repair

After explicitly running the repair statements in `data-trust.sql`, rerun the saved revisions:

- Customer ID required: Passed, 0 failures.
- Order ID unique: Passed, 0 failures.
- Amount in operating range: Passed, 0 failures.
- Known order status: Passed, 0 failures.
- Customer exists: Passed, 0 failures.
- Recent fixed-boundary order: Passed, 0 failures.
- Orders available: Passed, 0 failures.

The earlier failed aggregate runs remain visible until history is explicitly cleared. Clearing history does not delete definitions or project data.
