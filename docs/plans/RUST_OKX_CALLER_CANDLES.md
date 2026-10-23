# OKX caller adoption, slice 2: FetchCandles

Plan, 2026-10-24. Branch `feat/rust-okx-caller-candles` starts from slice 1
(`feat/rust-okx-caller-entry`, PR #1938, head `17ecffecfb7`), which is stacked
on the OKX signer PR #1773. Slice 1's plan
([RUST_OKX_CALLER_ADOPTION.md](RUST_OKX_CALLER_ADOPTION.md)) recommended candles
next: seven `lib.rs` sites, no history bridge (#1848), no socket code (#1771).
This document is the plan and the original RED/control evidence. Production
code is unchanged on this branch.

## Inventory on this branch

Re-derived by importing `sdk-rs/scripts/check-request-context.py` through
`importlib` from `sdk-rs` and replaying `tokens` / `receiver_start` /
`group_end` with token offsets to map each fingerprint to its line and
enclosing `fn`. The checker passes and the manifest equals the scan: OKX has
37 occurrences in 26 hash rows (history.rs 18/14, lib.rs 10/7, trading.rs 9/5).

In scope (7 occurrences removed, all in `venues/okx/src/lib.rs`):

| Fingerprint | Line | Function | Class | Replacement |
|---|---|---|---|---|
| `2b6c8768` | 675 | `request_failed` | post-send failure: `command_error` + `done "[]"` | `command_error_context(&root, msg)` then `done_context(&root, "[]")` |
| `c5a021da` | 1276 | `on_response` (Candles arm) | success: sorted rows | `validate_read_http_delivery(&root, &leg)` gate, then `done_context(&root, rows)` |
| `c873db6f` | 1277 | `on_response` (Candles arm) | row serialisation failure | `fail_context(&root, error)` |
| `2b6c8768` | 1481 | `on_command` FetchCandles | unknown market: empty success | admit, then `done_context(&root, "[]")` |
| `a39517b8` | 1489 | `on_command` FetchCandles | non-finite bound | admit, then `fail_context(&root, "invalid candle time bounds")` |
| `1cb580e2` | 1496 | `on_command` FetchCandles | unsupported interval | admit, then `fail_context(&root, interval.unsupported("OKX"))` |
| `a39517b8` | 1505 | `on_command` FetchCandles | `before > end` | admit, then `fail_context(&root, "invalid candle time bounds")` |

Touched but kept:

- **`c873db6f` lib.rs:1341 `on_sign_error`.** The arm is
  `History(..) | Candles { command, .. } => cx.fail(command, error)`. Candles
  get their own `fail_context` arm; the site stays for History (#1848).
- **`b8eef2a7` lib.rs:415 `list`.** Candles stop calling it (both the
  `candles` and the `history-candles` legs). It still serves markets, tickers,
  EEA private instruments, account snapshots, history and closed positions.
- **`2e921890` lib.rs:314 `sign_request`.** Candles never reach it: they call
  `list(.., private = false, ..)`, so `sign_http` is skipped. Unaffected.

Out of scope: history.rs (18, held for #1848), trading.rs (9, the write path),
the three remaining lib.rs rows above, and the #1771 socket (no inventory rows).
Live candle subscriptions (`SubscribeCandles`, socket `candle*` frames) are not
HTTP and are untouched.

Expected exact delta: lib.rs 10 → 3 occurrences and 7 → 3 rows
(`2e921890`:1, `b8eef2a7`:1, `c873db6f`:1). OKX goes 37 → 30 occurrences and
26 → 22 rows. history.rs and trading.rs are unchanged. The manifest prune is
exactly the removal of `1cb580e2`, `2b6c8768`, `a39517b8` and `c5a021da` from
lib.rs and `c873db6f` 2 → 1.

## Native behaviour to preserve (pinned by the controls)

- **Public and unsigned.** OKX `GET /api/v5/market/candles` and
  `/api/v5/market/history-candles` are public. The legacy path never signs
  them, even with primary or read keys and on EEA. The host follows the alias
  (`www.okx.com`, `openapi.okx.com` demo, `eea.okx.com` EEA and EEA demo).
  Demo aliases carry `x-simulated-trading: 1`. There is no `Content-Type` and
  no body.
- **Paging.** One command makes at most two physical reads. The first is
  `candles?instId=..&bar=..&limit=L&before=B&after=E` with `L = min(limit or
  300, 300)`. If that page is empty, or `to_ms` is older than 1440 bars
  (`force_history`), a second read goes to `history-candles` with the same
  bounds and `limit = min(L, 100)`. A forced fallback discards the recent page.
  Rows are sorted ascending by timestamp. With no explicit limit `before` is
  `from_ms` when given; an explicit limit always derives `before` from `end`.
  The month bar uses `sub_months`.
- **Timeout and retry.** The legacy reads have no engine deadline
  (`timeout_ms: None`; only the host's transport timeout applies) and are never
  retried. There is no venue timer or pacing.
- **Entry refusals.** Unknown market is a native empty success
  (`CommandDone "[]"`, no diagnostic). A non-finite bound and `before > end`
  fail with `invalid candle time bounds`; an unsupported bar (for example
  `8h`) fails with `OKX does not support 8h candles`. All three have kind
  `None`, outcome `None` and no venue code, and send no HTTP.
- **Post-send failures are soft.** Every failure after send (an envelope
  `code != "0"` → `OKX <code>: <msg>`, non-2xx → `OKX HTTP <status>: <body>`,
  a bad row → `malformed OKX list item`, transport → the transport text, a
  missing/broken list) emits one `CommandError` diagnostic and then
  `CommandDone "[]"`. No `CommandFailed`. The fallback leg fails the same way.
- **Pre-send signing failure is hard.** The `on_sign_error` Candles arm uses
  `cx.fail(command, error)` today.

## Replacement (landed APIs only)

Mirror the KuCoin Spot candle cutover (#1871, `bf6f3c3387f`) for the public
managed read, Phemex (#1878, `dbe3f7a9050`) for the unsigned managed route,
and slice 1 for the OKX-local shape. Every API below is on this branch:
`admit_http_command`, `http_leg_context`, `request_managed`,
`request_managed_with_headers`, `stream_response`,
`validate_read_http_delivery`, `done_context`, `command_error_context` and
`fail_context`. No `http-plans`, Plan, Preflight or new engine API is needed.

1. **Entry.** In `Command::FetchCandles`, admit first:
   `cx.admit_http_command(Policy { operation: Read, credential: None,
   timeout_ms: 20_000, pacing: None, retry: RetryPolicy::Never })`. On `Err`,
   return; the engine already published the typed failure. Then apply the
   four refusals above through `root` with unchanged text, and no added kind
   or outcome. `credential: None` keeps the read public on every alias.
2. **Physical legs.** `Pending::Candles` carries `root: Context`,
   `leg: Context`, `rows`, `invalid` and `read`. The `command` field goes.
   Each physical read, the first and the fallback, runs on its own
   `cx.http_leg_context(&root, same policy)`. The root is never sent. This is
   the Binance `trading.rs` root/leg shape and the engine's
   `request_context_read_delivery_tests` shape. A leg preserves the root's raw
   deadline, so the fallback inherits the remaining budget instead of a fresh
   20 s. If leg issuance fails, `fail_context(&root, reason.venue_error())`.
3. **Send.** Live aliases use `request_managed(leg, Get, url, None, pending)`.
   Demo aliases use `request_managed_with_headers(.., DEMO_HEADERS, ..)`; make
   `sign::dispatch::DEMO_HEADERS` `pub(crate)` and reuse it. On `Ok(id)` call
   `cx.stream_response(id, ArrayAt::EnvelopeField("data"))`, which keeps the
   legacy per-row `on_item` parse. On `Err((pending, reason))`, route to
   `on_sign_error`.
4. **Success.** In `on_item` and `on_response`, gate the Candles arm on
   `cx.validate_read_http_delivery(&root, &leg).is_ok()`, as KuCoin does.
   Then keep the fallback decision, sort, and `done_context(&root, rows)`.
   The fallback is issued from the first leg's `on_response`, on a new leg.
5. **Soft failures.** Override `on_http_failed` and route the Candles arm,
   with `request_failed`'s Candles arm, `on_stream_malformed`,
   `on_stream_error` and the envelope/status branch of `on_response`, to one
   helper: `command_error_context(&root, msg)` then `done_context(&root,
   "[]")`. Do **not** gate this helper on `validate_read_http_delivery`. At
   the deadline the leg is already expired, so that gate would refuse, and the
   command would hang. `command_error_context` validates completion authority
   without a deadline check.
6. **Hard pre-send failures.** Give `on_sign_error` its own Candles arm with
   `fail_context(&root, error)`. The default `on_http_preparation_failed`
   reaches it. This preserves today's hard `cx.fail`. As in slice 1, no
   `NotSent` outcome is added (Phemex adds one; see Q3).

Routing and egress: managed unsigned reads emit `ManagedHttp::Prepare` with
`RoutingIntent::Public` and `timeout_ms: Some(20_000)` before release, as
Phemex and KuCoin Spot do. Hosted callers that route private signed traffic
through a relay see candles as public-intent egress, which they already are.
No signing step, credential slot or crypto line is added, so
`check-signing.py` is unaffected.

Behaviour change, stated for callers: an unanswered candle read now ends at
20 s (first leg) or at the remaining original budget (fallback leg). It ends
with `CommandError "HTTP request timed out"` plus `CommandDone "[]"`, under the
preserved soft shape (Q1). More than 256 outstanding managed reads give one
typed `Capacity` failure before any HTTP. A late reply after a terminal is
dropped by the engine.

Architecture gate: **No.** This adopts the shared entry, managed-HTTP and
Context-completion choke points, and adds no venue-local retry, timer, ledger,
clock or admission.

## #1773 `STAGING_BOOTSTRAPS` entry (`venues/okx/src/lib.rs`)

The entry maps the pre-#1773 landing map (`1cb580e2`:1, `2b6c8768`:2,
`a39517b8`:2, `b8eef2a7`:1, `c5a021da`:1, `c873db6f`:1) to the staged map,
which adds `2e921890` (`sign_http`), `b1761cde` (legacy `get`, removed by
slice 1) and a second `c873db6f`.

- After this slice lib.rs is `{2e921890:1, b8eef2a7:1, c873db6f:1}`. That is
  a subset of the staged map, so the checker passes with no edit.
- **Nothing can be pruned.** The post-slice map still has `2e921890`, which
  the old map lacks, so the entry is still needed whenever a compared base
  holds the pre-#1773 map. Candles never reached `sign_http` (they are
  unsigned), so this slice cannot remove that row. It leaves when EEA private
  instruments, snapshots, history and closed positions leave `list(private)` /
  `sign_request`.
- Narrowing the staged map to the new subset is optional tightening of
  #1773's own entry. It is not needed for this slice and is left to #1773's
  owner. Deletion still follows #1773's landing (slice 1 owner decision 3).

## REDs and controls (original, unchanged production)

All in `sdk-rs/venues/okx/tests/candles_context.rs`, through the existing
`prepared()` fixture handshake (legacy actions pass through). Every test drives
`Engine<Okx>` events only and asserts observable actions.

Genuine REDs. Each fails today on an assertion:

1. `unanswered_candle_read_ends_at_the_shared_deadline`: no reply, fire
   armed timers to +20 s; expect one soft terminal (`CommandError "HTTP request
   timed out"` before `CommandDone "[]"`). A late reply adds no terminal and
   starts no fallback. Today:
   `assertion left == right failed: an unanswered candle read needs exactly one
   terminal by the 20 s shared deadline; left: 0, right: 1`.
2. `history_fallback_leg_keeps_the_original_deadline`: answer the first page
   empty at +10 s; the fallback must carry `timeout_ms: Some(10000)` and end
   softly at the original +20 s. Today:
   `assertion left == right failed: the fallback leg is bounded by the
   command's original deadline, not a fresh one; left: None, right:
   Some(10000)`.
3. `candle_reads_beyond_shared_capacity_are_refused_before_io`: 256
   outstanding reads, then one more must send nothing and fail once with
   `Error::Capacity`'s message/kind; an answered read frees a slot. Today:
   `the candle read beyond shared capacity must be refused before any HTTP`.
4. `candle_read_is_prepared_as_a_bounded_public_route`: with primary
   credentials, the raw actions must contain no candle `Http` before
   preparation, and exactly one `ManagedHttp::Prepare` with
   `RoutingIntent::Public`, `timeout_ms: Some(20000)`, the exact URL and no
   `OK-ACCESS-*`. Today:
   `a candle read must await managed route preparation instead of raw send`.

Controls, passing before and required to pass after:

- `candle_urls_limits_and_history_fallback_are_exact`: exact recent and
  history URLs, the 300/100 clamps, the `from_ms` rule, the forced history
  that discards a recent page, and ascending sort.
- `candle_entry_refusals_keep_text_kind_and_outcome_without_http`: unknown
  market gives `Done "[]"` with no diagnostic. NaN, ∞, `from > to` and `8h`
  keep their text with kind/outcome/venue code `None` and send no HTTP.
- `failed_candle_reads_keep_the_native_soft_empty_result`: envelope code,
  HTTP 500, bad row and transport failure on both legs each give one
  diagnostic before `Done "[]"`.
- `candle_reads_stay_unsigned_public_egress_on_every_alias`: public, primary
  plus read key, demo, EEA and EEA demo. Each has the right host, no
  `OK-ACCESS-*`, the demo header only on demo, and no body or Content-Type.
- `late_reply_after_a_terminal_is_inert`: a repeat reply or 500 after `Done`,
  and a stale first-leg reply after the fallback was issued, add nothing.

Existing coverage to keep green (switch to `prepared()` where it still uses
`actions()`, as slice 1 did for its fixtures):
`public.rs::candle_reads_use_ts_bounds_limit_and_historical_fallback`,
`month_candle_bounds_clamp_day_in_leap_february` and
`candles_on_business_and_all_sockets_watch_silence`.

## Implementation order

1. Harness: switch the three `public.rs` candle tests to `prepared()`, which
   is a pass-through today.
2. Steps 1–6 above in `lib.rs` (and `DEMO_HEADERS` visibility in
   `sign/dispatch.rs`).
3. Prune `request-legacy-sites.json` to the actual scan (the delta above).
4. Add the OKX candle bullet to `RUST_CUTOVER_CALLERS.md`.

Checks, from `sdk-rs` with
`PATH=$HOME/.rustup/toolchains/1.99.0-aarch64-apple-darwin/bin:$PATH CARGO_BUILD_JOBS=2`:
`cargo test -p safe-cex-venue-okx --no-fail-fast`;
`cargo test -p safe-cex-venues --features okx --test okx_signer --test okx_hosted_signing`;
`cargo fmt --check`; `cargo clippy -p safe-cex-venue-okx --all-targets -- -D warnings`.
From the worktree root: `python3 sdk-rs/scripts/check-request-context.py`
(with `GITHUB_BASE_REF=feat/rust-okx-caller-entry`), its `.test.py`, and
`check-signing.py`.

## CALLERS note (to add on implementation)

> **OKX `fetchCandles` (slice 2).** All four aliases. Candle reads stay public
> and unsigned, with the same URLs, limits, history fallback and row shape.
> They are now managed public reads with a shared 20 s budget per command; the
> history fallback uses what is left of it. A read that gets no answer ends with
> the usual error event `HTTP request timed out` and an empty `[]` result,
> which is the same soft shape as other candle failures. More than 256
> outstanding managed reads are refused with one typed capacity failure before
> any HTTP. Unknown market, invalid bounds and unsupported bars are unchanged.
> Live candle subscriptions are unchanged.

## Open questions for the owner

1. **Timeout shape.** Keep the native soft `CommandError` + `Done "[]"` for
   post-send failures, including the new deadline (recommended: exact native
   preservation; REDs 1–2 encode it in `assert_soft_empty`)? Or should the
   deadline become a typed `CommandFailed{Timeout}`, as FetchLeverage did? No
   landed candle conversion had a soft-empty legacy path (KuCoin preserved
   its hard `command_error` + `fail`).
2. **Unknown market.** Admit first and `done_context("[]")` (recommended; the
   Bitunix/KuCoin local-empty precedent), or answer before admission? Before
   admission would leave a numeric `cx.done` and keep one `2b6c8768` row.
3. **Typed refusal kinds.** KuCoin Spot added `Unsupported` + `NotSent` to its
   unsupported-interval refusal, and Phemex added `NotSent` to preparation
   failures. The control pins today's kind `None`/outcome `None`. Should we
   keep native (recommended for this slice) or type them now? If typed, the
   control changes with the owner's decision.
4. **Root/leg versus single context.** The plan sends every physical read on
   a leg of an unsent root (Binance shape). Reusing the admitted root for the
   first send and a leg only for the fallback (KuCoin/Phemex single-send
   shape plus one leg) also works. Is the uniform leg shape preferred?

## Decisions (claude-4yjlw0, slice owner; behaviour-preserving)

1. A deadline-expired candle read keeps today's soft shape: an error notice,
   then `[]`, asserted through `assert_soft_empty`. A typed Timeout would be a
   caller-visible change and needs its own decision.
2. Unknown market is admitted first, then answered `[]`. This prunes the
   `2b6c8768` entry-path site.
3. Refusal kinds stay untyped, as today (kind, outcome and venue code None).
   Adding kinds like KuCoin/Phemex did is a separate behaviour change.
4. Each read gets its own context: the recent page, then the history
   fallback, both bounded by the command's original 20 s deadline (Binance
   precedent).

**Implementation note on decision 4.** The recent page is sent on the admitted
command root, and only the history fallback opens a separate leg. A separate
leg for the first read would hold two live contexts per command and halve the
256-context registry cap (only 128 concurrent candle commands). That is the
KuCoin/Phemex single-send shape plus one leg. Each read still has its own
context under the command's original 20 s deadline.
