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
(done in #2990's head). For a managed logical read it is the earliest released
leg (`read_mark`), and venues report through the existing `orders_reconciled*`
calls: `orders_reconciled(None)`, `_since(None, _)` and `_except(&[], _)` are
complete; `Some(symbol)` and `_except` with unproven symbols are per-symbol,
covering the symbols proven.

A read already in flight when its rows are superseded (an older read landing
after a newer complete one) must not lower a newer cover, and must not raise
it. It is ignored for covers (its send is older than the cover it would touch).

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

1. Engine: per-symbol cover provenance in `Store` (§4.1) and section 23
   revision 1 written after ORDERS on every output (§4.2). Segmenter parent
   mapping (H6). Tests H2, H4, H5, H6.
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

## 8. Out of scope

- Positions and settings holds (`settings.held`), which have their own source
  re-read (`refreshAccountSettings`).
- Non-frame-fed engines. Their own reconcile reads already clear their own
  fences through `orders_reconciled*`, with read epochs.
