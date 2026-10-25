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
   bounds and the 100-page cap are unchanged. There is no page delay or page
   timer: the next page is requested from the response.
   - Intentional caller change, recorded in the area plan's step 5: a leg
     admission or queue refusal (preparation or policy) becomes strict
     (`history.reject`), so after an accepted full page it rejects with no
     partial result or marker. The reason is the shared owner's contract
     (`engine/src/history.rs`, `reject`): "Strict endpoint-policy rejection is
     not a recoverable failed page, even when earlier pages were accepted.
     Never publish its rows or marker." BloFin general history and OKX closed
     positions already follow it, and #2133 changed only page failures, not
     refusals.
   - A signing failure stays a recoverable page failure, as it is today and as
     in OKX closed positions: accepted rows plus one INCOMPLETE. History keeps
     its existing strict signing refusal
     (`header_preparation_refusal_is_strict_after_accepted_rows`). Each read
     keeps today's signing semantics, and only the leg-admission refusal
     changes. The change is recorded in CALLERS.
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

## Root entry and failure boundaries

These follow the Binance #2118 adoption.

- **Entry order.** The keyless `[]` answer and local refusals (unsupported
  client-ID lookup, invalid range, unknown market) come first, through
  `refuse_command` or a public `[]` root. Then `command_wait()`: Expired goes to
  `reject_expired_command(Read)` and an unstamped receipt goes to
  `reject_unstamped_command(Read)`; neither is ever defaulted. Only then
  `SelectedRead::command_root`, which fixes the identity for the whole query.
- **Root expiry.** The caller's wait bounds the whole walk. Root expiry is the
  engine's strict Timeout, with no rows and no marker. This is TS-visible and is
  recorded in CALLERS beside the per-page 20 s bound.
- **Leg admission.** `root.leg(..)` returning `Revoked` (root expired or
  retired) drops the state silently, because the engine owns that terminal. Any
  other leg error is a strict reject through `read_venue_error`.
- **Custody.** Every streamed item checks the root's and the owner's delivery
  before staging. A failed delivery has no body; one leg is in flight per query,
  so the failure belongs to that leg (stated at `on_http_failed`). Every
  terminal finishes its leg's Context.
- **Typed failures.** `on_http_failed` and `on_sign_error` route history and
  closed positions with their `VenueError` kept, so a 20 s page Timeout stays
  typed and a transport loss is `Network`. Private history used to report
  transport loss as `Internal` and now matches public trades.
- **Retirement.** BloFin keeps no venue-side state for these reads and has no
  page timers, so `read_root_retired` has nothing to remove.

Invariants for every site:

- exact URL, path and query bytes;
- the same header set (the recipe's signed headers plus `UNSIGNED_HEADERS`);
- the same Pending variant and decode path (`Pending::Account` now also carries
  its admitted read);
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

Measured before S1 (S1 had not landed). Tip `aec49a904bc` against the full
source `5cd98272caa` (tree `8843eebce99`), Rust 1.99.0, wasm-bindgen 0.2.129,
Binaryen 132, `BINARYEN_CORES=4`, shipped profiles:

| Bundle   | Before bytes | After bytes |  Delta |
| -------- | -----------: | ----------: | -----: |
| BloFin   |      999,498 |   1,005,092 | +5,594 |
| Combined |   14,036,202 |  14,040,650 | +4,448 |

The first-source pair (closed positions only, tip `34999f973f9`) was BloFin
+5,375 and combined +3,474. Both are within the +10K pre-S1 bar and the
venue budget (1,343,488).

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
- a closed-positions leg refusal after an accepted full page rejects with no
  partial or marker, while an ordinary later-page or signing failure still
  keeps the accepted rows;
- a later closed-positions page that times out in its 20 s window keeps the
  accepted rows with one INCOMPLETE;
- the inventory for these three files reaches 0.

A regression control (GREEN on the base, since legacy reads hold no Context)
pins leg hygiene. A refresh tick, then a WebSocket push that supersedes the
positions revision, then the late REST answer: nothing is published from it,
and its leg holds no owner capacity. This uses #2115's capacity probe against a
run where the read completes normally.

Existing controls keep their assertions. The native fixtures change only how
they drive the engine:

- managed reads release through Prepare/Bind/Claim at the event time
  (`Harness::handle`);
- `respond` answers at the harness start time;
- keyed fixtures build through the registry's trusted setup
  (`tests/trusted/mod.rs`);
- the parked-provider rig parks only after the account's own signed reads load.

Two leverage tests now pin the refusal at the settings reads that share the
identity. One history pin records transport loss as `Network`.

## Delivery

One PR on `feat/rust-blofin-read-funnel`. The RED commit comes first, then
source with the inventory shrink (`request-legacy-sites.json` −10) and a
CALLERS note. Local gates: `cargo test -p safe-cex-venue-blofin`, fmt, clippy
`-D warnings`, `scripts/check-bans.sh`, and the paired size. Then one push and
one PR run.
