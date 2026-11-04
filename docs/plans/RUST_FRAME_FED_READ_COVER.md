# Frame-fed fences: read covers and a bounded refresh ask

Status: design only, for review. No source change. Supersedes the approach of
#2990, which is parked as a draft (master keeps fence-until-real-change for this
release). Owner: fe7lgm. Reviewers: codex-hcmq3f (every finding below is theirs
or reproduced by them), 55ao7t.

## 1. What a fence is for, and what master does today

A frame-fed page engine (Hosted and local-worker accounts) sends writes itself,
but its store comes only from its data source's SCXF frames. When a placement's
outcome is unknown (`HttpFailed`, a timeout), the page seals an uncertain fence
over the symbol. Placing or amending there is refused, because a retry could
place a second order if the first one did land.

The page lifts the fence (`Engine::apply_frame`) when a source frame carrying
ORDERS is stamped at least `FENCE_GRACE_MS` (3 s, with a measured clock offset)
or `UNSYNCED_FENCE_GRACE_MS` (30 s, without one) after the fence's mark:
`cover = stamp - offset - grace`, and every mark at or below `cover` lifts.

Master's two weaknesses:

- **Stuck fence (liveness).** The source never saw the page's write. If the write
  never reached the venue, the source's orders never change, so no ORDERS frame
  comes and the fence stands until an unrelated order event, a resnapshot or a
  re-init. This was Bravulasco's report and the reason for #2990.
- **Late-read lift (safety, H2 below).** The frame stamp says when the source
  flushed, not how old its order rows are. Rows from a venue read sent before
  the lost write but landing more than the grace after it are stamped late
  enough to lift the fence, while saying nothing about the write.

The first costs the user a refused action. The second can cost a double
placement. Any fix must not trade the first for the second.

## 2. Hazards and acceptance tests

Each hazard below has a reproduction, and each becomes an acceptance test that
the implementation must pass (red on the stated head, green after). All of them
run against a real Bybit source engine feeding a real frame-fed page
(`venues/bybit/tests`) unless marked as an engine unit test. The `/tmp`
artifacts are codex-hcmq3f's originals; the tests are to be committed with the
implementation.

| # | Hazard | Repro (origin) | Acceptance test |
| --- | --- | --- | --- |
| H1 | A snapshot answers a fence ask: the source's store re-stamped "now", whose rows can predate the lost write when the source's stream has stalled | 55ao7t review of #2990 (d2cce24e15a) | `a_stalled_source_lifts_nothing_until_its_fresh_orders_read_lands` |
| H2 | A read sent before the lost write lands later than the grace; its delta frame lifts the fence. **Present on master.** | fe7lgm, `frame_fed_read_cover.rs` (e4c2442581c) | `a_read_sent_before_the_lost_write_cannot_lift_its_fence_however_late_it_lands` |
| H3 | A newer refresh ask coalesces into an older in-flight read; that read's frame lifts a fence sealed after it was sent | `/tmp/codex-hcmq3f-2990-old-refresh-repro.patch` | `an_older_refresh_read_cannot_lift_a_later_lost_write` |
| H4 | The same stale read rows, re-sent in `snapshot_frame` or `account_snapshot_frame`, lift a fence the delta correctly kept | `/tmp/codex-hcmq3f-2990-snapshot-repro.patch` | `a_snapshot_cannot_restamp_the_same_stale_read_rows` (both snapshot kinds) |
| H5 | Cover held per flush, not per row: partial read at 1, flush, partial read at 500, and the cover becomes 500 with no complete read | `/tmp/codex-hcmq3f-2990-recheck-boundaries.patch` (`review_partial_cover_survives_an_intermediate_frame`) | engine unit test, same name |
| H6 | The hosted server's `segment_frame` splits ORDERS from its cover; the ORDERS segment arrives first and lifts on its stamp | `/tmp/codex-hcmq3f-2990-segmentation.rs` | `orders_never_segment_apart_from_their_read_time` (hosts/native) |
| H7 | Unbounded source reads: 100 same-clock asks start 200 physical order reads | `/tmp/codex-hcmq3f-2990-recheck-boundaries.patch` (`review_repeated_same_clock_refreshes_have_bounded_physical_reads`) | same name |
| H8 | An older read lands after a fresh one: old read sent at 0 misses the write at 1; fresh read sent at 5 finds the order; the old read lands last and removes the row while cover 5 stays, and the next frame lifts with the order missing | codex-hcmq3f, #3000 design review (28a0b5d) | `an_older_read_landing_after_a_fresh_one_neither_removes_the_found_order_nor_lifts`, run three ways: the delta, `snapshot_frame` and `account_snapshot_frame` after it |
| H10 | Same-clock stream race: unknown placement at 1000; a read released at 5000; a stream event inserts the order at the same 5000; the delayed read lacks it and commits at 6000 (by time, 5000 is not older than 5000), removes the order, publishes cover 5000 and lifts fence 1000+3000 | codex-hcmq3f, #3000 re-check (48f1a88087c) | `a_stream_row_after_a_reads_release_at_the_same_millisecond_refuses_that_read` (keeps the order, publishes no cover, the fence stands). Reverse-order control: `a_read_released_after_a_same_millisecond_stream_row_commits` (the read is authoritative). Both run on a real same-clock source; neither may advance the test clock between the two events. |
| H9 | One scope's fresh send stands for the whole account: an old HIP-3 dex read sent at 0 plus a fresh core read sent at 5 yields account cover 5 and clears a HIP-3 fence. The same shape applies to Bybit products with independent read roots. | codex-hcmq3f, #3000 design review (28a0b5d) | Hyperliquid `an_old_dex_read_keeps_the_account_cover_and_its_fence`; Bybit `an_old_product_read_keeps_the_account_cover` |

Controls that must stay green:

- The earliest released leg of a managed logical read is its send time
  (`review_cover_uses_the_first_released_leg`).
- A refresh that never lands lifts nothing (`review_refresh_timeout_releases_only_the_source_slot`).
- A fresh read finding the lost order brings its row to the page; one finding
  none lifts the fence.
- A second write lost during the grace is asked for again after a partial lift
  (from #2990, red on its earlier head).
- Wire: JS `wire.mjs` and older Rust ingest parse a frame with the new section
  and ignore it (codex-hcmq3f: 41 wire tests pass; standalone JS parses ORDERS+23).

## 3. Principle

**Only a venue read can prove an order absent.** A stream proves presence (an
order event), never absence: a write that never reached the venue produces no
event at all. So a fence, which waits on "did this write land?", may lift only
on a read of that symbol sent after the write plus the grace. The frame stamp
proves nothing about absence and stops being a lift condition. That makes
liveness depend on reads actually happening, which §5 provides with a bounded
ask.

## 4. Read covers on the wire

### 4.1 Source: provenance per symbol, kept with the rows

The source `Store` keeps, beside its order rows:

- `account_read: Option<f64>` is the send time (source clock) of the last
  **complete** account-wide orders read. It speaks for every symbol, including
  "no order on this symbol".
- `symbol_read: HashMap<symbol, f64>` holds per-symbol reads newer than
  `account_read` (Phemex reads one symbol at a time; others page by settle coin
  or product).

The cover of symbol S is `max(account_read, symbol_read[S])`. It only moves
forward on a read that spoke for S. A complete read clears `symbol_read` and
sets `account_read`. A partial read sets only its symbols. Stream events don't
touch either (H5: no flush or stream event can raise a cover).

Send time is the existing `order_read_tokens` epoch pair extended with time
(done in #2990's head). For one managed logical read, it is the earliest
released leg (`read_mark`).

**An account cover is the minimum over every constituent scope (H9).** Many
complete reads are assembled from independent scopes, each with its own read
root and send time:

- Hyperliquid: core plus every HIP-3 dex. `account.rs` keeps only an epoch per
  dex in `clean_dexes`, and the final scope calls
  `orders_reconciled_since(None, minEpoch)`.
- Bybit: one read root per product or settle coin.
- Paged venues: one root per page chain.

The account cover is the **minimum retained proven send time across all of
them**, not the send time of whichever scope finished last. A final callback's
own token or timestamp is never account-wide proof.

Each scope therefore reports its own send time when it commits, and the
account-wide report names the scopes it joins. The engine takes the minimum of
the retained per-scope sends. A venue that cannot attribute a send time to each
scope produces per-symbol covers only, for the symbols its scopes proved, and no
account cover. Today's `orders_reconciled_since(None, epoch)` and
`_except(&[], epoch)` calls carry an epoch minimum but no per-scope time, so
step 1 extends them (or adds `orders_scope_read(scope, sent)` beside them), and
each venue is audited.

**Rows and cover commit together, and an older read never rolls either back
(H8).** A cover is only as good as the rows beside it. Take an old read sent at
0 that misses a write at 1, and a fresh read sent at 5 that finds the order. If
the old read lands last and replaces the rows, the order disappears while
cover 5 stays, and the next frame lifts the fence with the order missing.

- Every row replacement from a read goes through the store's existing atomic
  commit (`begin_orders_snapshot` / `commit_orders_snapshot`, whose tokens order
  by begin). It writes the rows and that read's cover in the same call.
- **Ordering uses a causal sequence, never wall time (H10).** `Date.now`
  timestamps tie, so a read and a stream event in the same millisecond can't be
  ordered by time. The store's monotonic `orders_revision`, which advances on
  every row change, is the order key.
  - A read records the revision current at its release (its earliest released
    leg).
  - Every stream row mutation advances the revision and records it as that
    symbol's last-mutation sequence.
  - The commit is refused for every symbol (or scope) whose last-mutation
    sequence is greater than the read's release sequence, i.e. whose rows
    changed after the read went out, however close the clock times. Its rows are
    dropped and its cover is not applied.
  - A read released after a mutation, even in the same millisecond, has a
    greater sequence and commits normally.
- Send **times** remain only for what crosses the wire, the covers compared
  against fence marks on the page. Every accept or refuse decision on the source
  is by sequence.
- A read already in flight when newer rows land therefore can neither lower a
  cover, nor raise it, nor replace the newer rows.
- Venues that write rows outside `commit_orders_snapshot` today (direct
  `set_orders`, `upsert_order` and `remove_orders` from read handlers) move onto
  it in step 1. The audit lists each one.

### 4.2 Wire section `ORDERS_READ` (23)

Revision 0 (as committed on the parked branch): one f64. **Replace it with
revision 1, unshipped in master, so no compatibility concern**:

```
u32 count, f64 account_read (NaN = none),
count × (u32 symbol string ref, pad) then count × f64 symbol cover
```

Rules:

- Written by `Store::write` **immediately after ORDERS**, whenever ORDERS is
  written (deltas, `snapshot_frame`, `account_snapshot_frame`), always carrying
  the full current cover table, not just changes. The table is small (symbols
  with a newer-than-account read only). This closes H4 and H5.
- The hosted segmenter maps 23 to parent 7 (`section_parent`), so the two never
  split (H6). A frame where 23 does not directly follow 7 is a writer bug; the
  segmenter already refuses an orphaned dependent.

### 4.3 Page: lift per symbol, on covers only

`apply_frame`, for each uncertain symbol S whose fence mark is `(token, t)`:
lift S when `cover(S) - offset >= t + grace`. Grace stays: it absorbs clock
skew and the write-in-flight race (a read sent after the write but processed by
the venue before it). With no measured offset, use the unsynced grace as today.

Fence marks today are global (`fence_marks: Vec<(token, time)>`, lifted by
`reconciled(None, token)`). They become per symbol. `order_locks.uncertain`
already keys epochs by symbol; the mark time is recorded per seal, and the lift
calls `reconciled(Some(S), token)`. An uncertain "" (all symbols) fence lifts
only on an `account_read` cover.

### 4.4 Rollout and mixed versions

Hosted servers and pages deploy separately. A page must not keep a fence for
good because its source predates covers.

- The source advertises covers in its capability record (a new capability
  bit, carried in snapshot frames' CAPABILITIES section).
- A page whose source advertises covers lifts **only** on 23. ORDERS without 23
  from such a source lifts nothing, so a segmented frame can't sneak past even
  if a future writer regresses.
- A page whose source does not advertise covers keeps master's stamp rule
  (including H2), unchanged. Old pages ignore section 23 (verified) and keep
  master's behavior with new sources.

## 5. Bounded refresh ask

### 5.1 Page

On sealing an uncertain fence on a venue that supports it, the page emits
`orders.held { symbols, liftAfterMs }`. The wrapper calls
`refreshAccountOrders(symbols)` once `liftAfterMs + 500 ms` has passed, through
the existing worker-method bridge. The hosted server maps it to
`refreshOrders { symbols }`; NX needs no change, as in #2990.

`liftAfterMs` is timed for the oldest standing mark, re-announced after a
partial lift (from #2990, gap 2), and reset on a full lift (gap 3).

### 5.2 Source

Per account: at most **one refresh read in flight** plus **one pending
follow-up**.

- An ask opens a newer epoch (`order_locks.advance()`) and merges its symbols
  into the pending set. With no read in flight, it starts one read (for the
  pending symbols, or account-wide where the venue reads that way) and moves the
  pending set to in-flight.
- With a read in flight, an ask only raises the pending epoch and merges
  symbols. It starts nothing (H7).
- When the in-flight read lands: its cover updates (§4.1). If a pending set
  exists whose epoch is newer than the landed read's send epoch, start exactly
  one follow-up read for it. A read older than the ask can never answer it (H3);
  it can still contribute an older cover, which can't lift a newer fence because
  §4.3 compares send times.
- A read that fails or never lands releases the in-flight slot after the read
  root's own deadline. The pending set survives and is retried once with
  backoff; nothing lifts (control above).
- Forced ORDERS: when a refresh read lands with unchanged rows, the next frame
  still writes ORDERS and 23, so the page sees the new cover (the
  `mark_orders_read` mechanism from #2990).

### 5.3 Venues

All frame-fed venues: binance (4 aliases), bingx, bitget (3), blofin, bybit (4
incl. HyroTrader), hyperliquid (+spot), lighter, okx (4), paradex, phemex.

- Every one except Phemex reports complete reads from its ordinary orders-read
  handler (binance `settle_orders`, bingx/bitget/blofin/okx/hyperliquid
  `account_response`, bybit `finish_orders`, lighter `commit_orders`, paradex
  `apply_response`), and its `reconcile_uncertain(None)` reads account-wide.
  Each gets `refreshes_orders = true`.
- **Phemex** reports per symbol (`orders_reconciled(Some)`), and its
  `reconcile_uncertain` reads only symbols it owes itself (`account.reconcile`).
  It needs a venue change: `reconcile_uncertain(Some(symbols))` from a refresh
  adds those symbols as owed reads (same per-symbol read and pacing), so a
  refresh produces per-symbol covers. With §4.3's per-symbol lift, Phemex fences
  then lift on their own symbol's read. Until that venue change lands, Phemex
  stays `refreshes_orders = false`, and its fences keep master's behavior via
  §4.4 (its source doesn't advertise covers).
- In the test harness with no private stream connected, a started Bybit source
  issues no orders read after startup (only `account/info` every 30 s), so its
  liveness depends on the refresh ask. Whether a live Bybit source polls orders
  is to be confirmed in step 3. Venues
  that poll orders also get covers from those polls for free.

## 6. Master's late-read hazard now (H2): is a smaller fix needed sooner?

**Exposure.** All of these must hold:

- The page loses a write whose outcome is unknown, and the write did reach the
  venue.
- A source orders read was already in flight when the write went out, or went
  out before the venue applied it.
- That read lands more than 3 s after the write's mark.
- No other ORDERS frame shows the order in between: the source's private stream
  would normally push it within the grace, and that frame carries the order row,
  so it is safe.
- The user retries.

In practice the stream push is the main protection, and the hazard needs a
stalled or lagging source stream plus a slow REST read. It's rare, and its
outcome is a double placement.

Options for a smaller, sooner fix:

1. **Grace no shorter than a read's maximum lifetime.** A read landing at stamp
   `s` was sent no earlier than `s - L`, where L is its maximum lifetime. With
   `cover = s - offset - max(grace, L)`, a lifting read was sent after the
   mark. No wire change; only the page engine changes. Cost: every lift waits
   L instead of 3 s. Caveats:
   - L is not a hard bound today. The JS host gives each physical request
     20 s (`transport/engine.ts`, `timeoutMs ?? 20_000`), but a multi-page
     logical read (Bybit pages by settle coin and cursor) applies rows after its
     last page, so its earliest page can be older than 20 s.
   - The native (hosted) host's request timeout is not verified here.
   Making this sound needs a logical-read deadline that every frame-fed source
   enforces, and that is a source change anyway.
2. **Per-flush delta cover** (#2990 at 85f1fb51414). It fixes H2 for deltas
   only, but carries H4, H5 and H6, so it can't ship alone.
3. **Nothing until this design lands**, keeping master's behavior.

**Recommendation: option 3, with this design as the next engine change.**

- Option 1 needs the same source-side read-lifetime bound this design needs,
  and slows every lift roughly sevenfold. Shipped alone, it buys little over
  doing the design.
- Option 2 is unsafe alone.
- The hazard is pre-existing, needs a stalled stream plus a slow read plus a
  retry, and is no worse after the #2990 park.

If the overseer wants a sooner stopgap anyway, option 1 with L = 60 s is
the only one that strictly reduces risk, at the cost of 60 s lifts. A hard
logical-read deadline of 60 s on frame-fed sources would make it sound.

## 7. Implementation order

1. Engine plus a venue audit:
   - Per-symbol cover provenance in `Store`, with rows and cover committed
     atomically through `commit_orders_snapshot`, and no rollback by a read
     released before the rows last changed, ordered by `orders_revision`
     sequence (§4.1, H8, H10).
   - Per-scope send times with a minimum account cover (H9), and every
     frame-fed venue's read handlers moved onto that path.
   - Section 23 revision 1 written after ORDERS on every output (§4.2), and the
     segmenter parent mapping (H6).
   - Tests H2, H4, H5, H6, H8, H9, H10.
   - Implementation notes from codex-hcmq3f's clearance (d9905b4f137):
     - The revision rule covers read commits too, not only stream mutations.
       An accepted read commit advances `orders_revision` and records it as the
       last-mutation sequence of every symbol it wrote, so a still-older read
       is refused against it.
     - Per-symbol mutation provenance lives beside the rows, not inside them.
       It must survive a row's removal and an empty snapshot or read result: a
       symbol with no row keeps its last-mutation sequence, so a missing row
       never erases the proof that refuses an older read.
2. Engine: per-symbol fence marks and the cover-only lift behind the capability
   bit (§4.3, §4.4). Mixed-version tests: an old source keeps the stamp rule; a
   new source's ORDERS without 23 lifts nothing.
3. Engine and hosted server: the bounded refresh ask (§5.2), `refreshOrders`
   with symbols. Tests H1, H3, H7 and the controls.
4. Wrapper: the `orders.held` and `refreshAccountOrders` ask (from #2990, with
   symbols).
5. Phemex: refresh-requested symbols become owed reads, then
   `refreshes_orders = true`.

Each step is its own PR and carries its acceptance tests. Steps 1 and 2 are
safe alone, since without asks nothing new lifts. Step 3 is the liveness fix
Bravulasco needs.

Wasm grows on every frame-fed family. Budget raises go in the overseer's cap PR.

## 9. Step 2 notes: the page lift rule and the capability bit

Design notes for step 2, written while steps 1a (#3004) and 1b (#3005) are in
review. No code yet. They refine §4.3 and §4.4.

### 9.1 The capability bit

- `Capabilities::order_read_covers`, flag bit 30 of the CAPABILITIES record's
  first word. Bits 0–29 are in use; 14 is reserved.
- It's written in every `snapshot_frame`, which is what a page always starts
  from. Deltas before the first snapshot are dropped. The hosted server sends
  `snapshot_frame` (`hosts/native/src/server.rs`), and the local worker forwards
  its own.
- **A venue sets it only once its source both commits covers (step 1b..n) and
  answers a refresh ask (step 3).** Covers without the ask would make a
  migrated page strictly slower to lift. Bybit, for one, issues no periodic
  orders read: a fence would wait for the next startup or reconnect read.
  So step 2 ships dormant, with no venue setting the bit, and each venue turns
  it on in its step-3 PR.
- Rust ingest learns to read just this bit from CAPABILITIES. Today it ignores
  the section; `Applied` gains `read_covers: Option<bool>`, set only by a frame
  carrying the section. The page keeps the bit from its latest snapshot. It
  never infers it from section 23's presence: the write is gated on a cover
  existing, so a new source sends no 23 before its first read.
- The JS decoder names only known flags (`CAPABILITY_FLAGS` in `wire.mjs`), so
  bit 30 is ignored there. A test pins that.

### 9.2 Parsing section 23 on the page

- Revision 1 only: u32 count, pad, f64 account (NaN = none), count u32 string
  refs, pad, count f64.
- Validation: count is bounded by the section length; refs resolve in the
  frame's string table; every cover is finite. A malformed section, or another
  revision, is read as **absent**, and with the bit set, absent lifts nothing.
  It is never an ingest error, so a source bug can't stall the page's frames.
- `Applied` gains `orders_read: Option<Covers { account: Option<f64>, symbols:
  Vec<(String, f64)> }>`. Covers are absolute, a full table each time, so the
  page stores nothing between frames.

### 9.3 The lift rule

`apply_frame`, when the frame applied ORDERS:

- **The source's last snapshot lacked the bit:** master's stamp rule,
  unchanged. This covers old sources and venues not yet migrated.
- **It had the bit, but this frame has no valid 23:** lift nothing. That
  includes snapshots, a segment split by a regressed server, and a malformed
  section.
- **It had the bit, and 23 is present:** for each uncertain symbol S
  (`order_locks.uncertain`, keyed by symbol with the epoch that sealed it):
  - Its mark time `t(S)` is the time of the first fence mark whose token is at
    or above S's epoch. `fence_marks` stays a token→time list, recorded as
    today when a seal happens.
  - `cover(S) = max(account, symbols[S])`, or none.
  - Lift S when `cover(S) - offset >= t(S) + grace`, through
    `order_locks.reconciled(Some(S), epoch(S))`. Grace and offset are the same
    as today (3 s with a measured offset, 30 s without). The placing guard in
    `reconciled` still applies.
  - The all-symbols fence (`""`) lifts only on the account cover. A symbol
    cover can't speak for every symbol.
- Marks no uncertainty still uses are pruned after the lift, as today.
- The `orders.held` re-announce for fences still standing (gap 2, from #2990)
  moves into step 3 with the ask.

### 9.4 What step 2 does not change

- No source behavior: covers come from step 1, asks from step 3.
- Non-migrated venues and old sources keep the stamp rule exactly, H2 included.
- Old pages ignore 23 and bit 30, and keep master's behavior against new
  sources.

### 9.5 Acceptance tests for step 2

All page-level tests run on a real Bybit source feeding a real frame-fed page,
with the bit forced on in the test harness, since step 2 ships dormant:

- **H2:** a read sent before the lost write lands more than 3 s later; its
  ORDERS frame with 23 lifts nothing.
- **H4:** the same stale rows in `snapshot_frame` and in
  `account_snapshot_frame`, which carries no CAPABILITIES (the bit comes from
  the last snapshot), lift nothing.
- **H8 and H10 at page level:** the source's refused reads publish no cover,
  so the page's fence stands.
- **A fresh read lifts:** sent after mark + grace, with no order found, it
  lifts the fence. With the order found, the row arrives and the fence lifts.
- **Per symbol:** a BTC read cover lifts BTC's fence and leaves ETH's.
- **The all-symbols fence** lifts only on an account cover.
- **Mixed versions:**
  - A source without the bit keeps the stamp rule (H2 reproduces there, as on
    master).
  - A source with the bit sending ORDERS without 23, or with a malformed 23,
    lifts nothing.
  - JS ignores bit 30.
- **H6 at page level:** a frame segmented by the hosted server lifts only via
  the segment that carries ORDERS and 23 together.

## 8. Out of scope

- Positions and settings holds (`settings.held`), which have their own source
  re-read (`refreshAccountSettings`).
- Non-frame-fed engines. Their own reconcile reads already clear their own
  fences through `orders_reconciled*`, with read epochs.
