# BloFin legacy read funnel

Base: `feat/rust-rewrite` `c3570bf762f`. Owner: `claude-kuhwxi` (overseer
assignment, 2026-10-25). Reviewers: `claude-4yjlw0` (history) and
`claude-dazzy3` or `claude-fe7lgm` (source). Landing: `claude-8x5tm1` exact-tree
GO.

This executes steps 4 and 5 of the area plan
[RUST_BLOFIN_REQUEST_CONTEXT.md](RUST_BLOFIN_REQUEST_CONTEXT.md) (lane
`codex-9czdjh`, reassigned) for the private reads. Its prerequisites are now
landed: the HMAC dispatch recipe (#2084), the Context-bearing `ReadCompletion`,
and per-page managed legs under one Read root (as OKX and Phemex use them).

## Scope

The request inventory (`sdk-rs/scripts/request-legacy-sites.json`) lists 18
BloFin occurrences. This plan removes the 10 that are reads:

| File         | Inventory | Actual legacy call                                     | Why the rest count                                                                                                                        |
| ------------ | --------: | ------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `history.rs` |         7 | `history_page`: one `cx.request_streamed`              | Its signing-header closure captures `cx` (`fill_signing_entropy`), so the checker freezes the six `matches!` macros in that tainted scope |
| `closed.rs`  |         1 | `request_closed_page`: one `cx.request_streamed`       | none                                                                                                                                      |
| `account.rs` |         2 | `private_read`: `cx.request_streamed` and `cx.request` | none                                                                                                                                      |

The 8 `trading.rs` occurrences are writes. They need a Mutation Plan, which is
a large combined-size add, so they wait for opt-s and S1 under the overseer's
size order. `Blofin::headers` stays because trading still uses it.

## Shapes

All three reads already have native completion owners, so only the request
edge changes. Every page or read becomes one ordinary managed leg, signed by
the existing `crate::dispatch_signing::Http` recipe (#2084, managed
FetchLeverage). Nothing reaches `fill_signing_entropy` from a venue closure, so
the six frozen macros leave the inventory as well.

1. **History queries** (`history_page`): fills, order history and income.
   - Today the query uses the legacy `Completion::new(command)`
     (`history.rs:293`). It adopts a caller Read root at the exact receipt
     (`ReadCompletion::from_context`), as OKX history does. Each page is a 20 s
     child leg at its actual send, capped by the caller root's deadline (as
     Bybit's history slices are), with `RetryPolicy::Never` and no pacing. The
     keyless `[]` short-circuit and local refusals keep their order.
   - Selection is Secondary else Primary for GET, as `headers()` chose today,
     through `SelectedRead`.
   - Public trades never reach `history_page`: `start_query` routes them to
     the managed `start_public_trades` (`history.rs:287`). The dead
     `PublicTrades` arms in `history_page` (the unsigned-headers branch and the
     endpoint row) are removed as orphaned.
   - The paging cursor, ascending/stall rules and accepted-page completion
     (shared owner; failure before accepted rows rejects, after them partial
     plus INCOMPLETE) are unchanged.
2. **Closed positions** (`request_closed_page`): the same leg shape on the
   existing `ReadCursorHistory` owner, as OKX closed positions do. Cursor,
   bounds, the 100-page cap and page delay are unchanged.
   - Intentional caller change, recorded in the area plan's step 5: a
     preparation, policy or signing refusal becomes strict
     (`history.reject`). Today it routes through the recoverable
     `history.finish`, so after an accepted full page it now rejects with no
     partial result or marker. The reason is the shared owner's contract
     (`engine/src/history.rs`, `reject`): "Strict endpoint-policy rejection is
     not a recoverable failed page, even when earlier pages were accepted.
     Never publish its rows or marker."
   - That is the rule BloFin general history (`request.reject` on a refused
     leg) and OKX closed positions (`history.reject` on a refused leg) already
     follow. #2133 changed only page failures, not refusals. Closed positions
     are the one BloFin read that deviates today, and this aligns them. The
     change is recorded in CALLERS.
   - An ordinary later-page transport/HTTP/parse failure still recovers the
     accepted rows with one INCOMPLETE.
3. **Account reads** (`private_read`): startup margin and position-mode
   settings, the positions and balance refresh, and open and trigger order
   pages.
   - These are commandless, so each is one `SelectedRead::background` read, as
     ApeX's snapshot legs are. Each has a 20 s window. A failure enters
     `account_failed` and its existing retry and backoff.
   - Selection is credential-aware with the same Secondary-for-GET rule.
   - A selection or admission refusal maps to today's `account_failed` text and
     path. Startup's missing-passphrase rejection stays where it is.
   - The joined refresh families (positions plus balance, and open plus
     trigger order pages) keep their native generation join. Each leg is its
     own Background read, and no venue-local admission core is added.
   - Every terminal finishes its leg's Context: a response, a failure, and a
     late answer the native revision check drops as stale. A superseded leg
     never lingers until engine expiry (the Bybit #1971 finding, fixed in
     #2115).

Invariants for every site:

- exact URL, path and query bytes;
- the same header set (the recipe's signed headers plus `UNSIGNED_HEADERS`);
- the same Pending variant and decode path;
- no Store-effect, revision or startup-failure change;
- no engine, host, provider recipe, signing inventory or credential inventory
  change.

## Size gate

This is a pre-S1 add, so combined must stay at or under +10K. S1 is in its
landing slot, so measure on a tip that includes S1 if it has landed, and
otherwise state which side of S1 the pair was taken on. Freeze a first-source
paired measurement on that tip (Rust 1.99.0, Binaryen 132,
`BINARYEN_CORES=4`, shipped profiles), BloFin venue plus combined, before the
full source. If combined exceeds +10K, stop and report to the overseer. No
budget is raised. After the measurement, delete the scratch `target/`.

## Original controls (RED first)

Each RED fails on the base:

- a signed history page emits a managed Prepare for its exact URL, never a
  legacy `Action::Http`;
- a closed-positions page does the same;
- each account read (margin, mode, positions, balance, open orders, trigger
  orders) does the same as a Background read;
- a Secondary read slot signs history, closed and account GETs, and a missing
  passphrase or credentials refuses with today's text and no request;
- history accepted-page partial and first-page rejection are unchanged through
  the managed leg;
- a closed-positions preparation refusal after an accepted full page rejects
  with no partial or marker, while an ordinary later-page failure still keeps
  the accepted rows;
- the inventory for these three files reaches 0.

A regression control (GREEN on the base, since legacy reads hold no Context)
pins leg hygiene. A refresh tick, then a WebSocket push that supersedes the
positions revision, then the late REST answer: nothing is published from it,
and its leg holds no owner capacity. This uses #2115's capacity probe against a
run where the read completes normally.

Existing GREEN controls stay byte-exact: the history, closed-positions,
account, startup and HMAC dispatch fixtures.

## Delivery

One PR on `feat/rust-blofin-read-funnel`. The RED commit comes first, then
source with the inventory shrink (`request-legacy-sites.json` −10) and a
CALLERS note. Local gates: `cargo test -p safe-cex-venue-blofin`, fmt, clippy
`-D warnings`, `scripts/check-bans.sh`, and the paired size. Then one push and
one PR run.
