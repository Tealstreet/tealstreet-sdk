# Binance history under shared Read authority

Plan only, 2026-10-25. Owner: claude-4yjlw0; overseer: claude-ddi2il.
Reviewer: claude-fe7lgm or claude-dazzy3, whichever is free.
Branch `feat/rust-binance-history-context` from landing `9bba5373dbb`. No source until the overseer rules.

## Scope and inventory

The scope is the Binance history and lookup funnel: `venues/binance/src/history.rs`, plus the `fn signed` request in `lib.rs` that only history uses. FetchCandles local refusals, trading and account reads are out of scope.

| File | Site | Fingerprint | Occurrences |
| --- | --- | --- | ---: |
| `history.rs` | signing-failure terminal (:295) | `5d6691e3` | 1 |
| same | Spot/COIN-M lookup or PublicTrades unsupported (:307) | `153ef00b` | 1 |
| same | Spot/COIN-M history unsupported (:310) | `2263b7c3` | 1 |
| same | `done []`: keyless, Fills or OrderHistory with no symbol (:326, :340, :366) | `2b6c8768` | 3 |
| same | unsigned PublicTrades request (:432) | `f859db20` | 1 |
| same | invalid client order ID (:461) | `a2e05934` | 1 |
| same | unknown market, `UNKNOWN` (:465) | `896c2dc2` | 1 |
| same | cached-order hit and lookup response `done` (:478, :619) | `c5a021da` | 2 |
| same | serialisation or lookup failure (:479, :620) | `c873db6f` | 2 |
| same | lookup with no primary or `public_data_only` (:485) | `6a3d37fd` | 1 |
| same | signed primary lookup GET (:506) | `abe91b4d` | 1 |
| `lib.rs` | `fn signed` request (:412), whose only production caller is history | `29866cd3` | 1 |

Expected inventory change:

- **`history.rs`:** 11F/15O to 0F/0O.
- **`lib.rs`:** loses `29866cd3` only. FetchCandles `088924f3` and `c873db6f` stay.
- **`secondary_tests.rs`:** its direct `signed()` callers are rewritten against the managed path. Signing fingerprints there must not grow.
- **Credential inventory:** unchanged. History selects from the existing `Slots<Identity>` (`self.credentials`), with no new `read_credentials` site.

## Design: the landed caller Read root (WEEX shape)

No engine cargo feature is needed. `admit_caller_read_root`, `SelectedRead`, `ReadRoot::leg`, `http_leg_context`, `ReadCompletion` and `ReadHistoryRows` / `ReadRangeHistory` are all ungated (OKX ships them with only `order-locks` and `managed-sockets`).

1. **Entry.** In `start_query`, keep native precedence: unsupported product first (Spot/COIN-M history, Spot PublicTrades), then local validation.
   - Local refusals and `done []` results use a command-owned Read Context: `admit_http_command(Read)`, then `fail_context` / `done_context`. This is the existing `reads.rs` pattern.
2. **Root.** Use `cx.command_wait()`, then:
   - `Expired` goes to `reject_expired_command(Read)`.
   - `None` goes to `reject_unstamped_command(Read)`. A missing receipt is never defaulted, as ruled for OKX.
   - Otherwise build a `SelectedRead` root.
   - **Role.** Fills, orders and income use `ReadRole::Read`: the read key when configured, else Primary, which is native. The lookup uses `Primary { reason: ClientOrderLookup }`. The identity is fixed for the whole query.
3. **Pages.**
   - **Signed pages:** each physical page is one `root.leg(ReadLegPolicy { timeout_ms: 20_000 })` queued through `request_signed_managed`, with the existing HMAC recipe (`sign::Http` with recvWindow and header policy).
   - **Unsigned PublicTrades:** one `http_leg_context(root, Read, no credential)` through `request_managed`.
4. **Completion.**
   - Fills and orders use `ReadHistoryRows`. Ranged income uses `ReadRangeHistory` with the native row identity (`tranId|category|incomeType|income`), the `income_incomplete` marker, 7-day windows and the 2 s chunk timer.
   - Lookup uses `ReadCompletion`. `-2013` with a 2xx or 400 status is the only definitive `ABSENT`; everything else stays strict.
   - Rows are admitted only after `validate_delivery`, then `finish_http_context`.
5. **Hooks.**
   - An `on_http_failed` override keeps the typed Timeout. Binance has none today, so the default would drop the kind.
   - `read_root_retired` drops the queued native state for that owner.
   - `Pending::History` is boxed.

The reference implementation is `venues/weex/src/history.rs`: `history_root` :400-439, `send_history_page` :503-603, the range and single income owners :787-836, `history_next_income` :1010, and the soft and strict endings :1421-1461.

## Native behaviour to preserve

- **Endpoints and parameters.** USD-M `/fapi/v1/*` and PM `/papi/v1/um/*`; public trades go to `urls.rest` (COIN-M `/dapi`).
  - The `limit` is clamped to 1..1000, with oldest-first 7-day fill windows and no global cap.
  - Duplicate IDs are kept, and income is sorted by `created_time` descending.
- **Unsupported and empty results.** Unsupported answers precede credentials. Keyless and symbol-less queries stay `[]` with no private I/O.
- **Lookup.** The validated client-ID grammar is unchanged (max 32 for PM, 36 otherwise). An unknown market gives `UNKNOWN`. The cache shortcut applies only without a secondary key.
- **No retries.** There is no history retry; `observe_weight` pauses never gate history.
- **Failures.** The first page failing gives a typed failure with no marker. A later page failing keeps the accepted prefix plus one `history.incomplete` marker. Signing refusal is strict NotSent, with timers cleared.

## TS-visible changes (recorded in `RUST_CUTOVER_CALLERS.md` at production)

- **Per-page bound.** Each physical page ends at 20 s as a typed Timeout. Native had no request deadline.
- **Root expiry.** The caller command wait bounds the whole walk. `Engine::handle` stamps the default 120 s. Ranged income at 2 s per 7-day chunk crosses 120 s at roughly 14 months of range. There, root expiry is a strict Timeout with no rows and no marker, matching the landed WEEX and Bybit behaviour. Callers raise `rustCommandTimeoutMs` for long walks.
- **Unstamped commands.** A command with no wait receipt is refused NotSent (`reject_unstamped_command`); it is never defaulted.
- **Provider refusal.** A refusal on a later page is strict through the root, as OKX closed history was ruled.

## Size (measured at tip `ba6fb191239`; local raw, function names excluded; CI factor about 1.0)

- **Binance family:** 1,235,305 bytes in CI, against the 1,343,488 venue budget (108,183 of room).
- **Combined bundle:** 13,894,263 bytes in CI, against 14,155,776 (261,513 of room).

| Item | Binance family | Combined |
| --- | ---: | ---: |
| Engine features needed | none | none |
| Avoided: `http-plans` linked, no use (measured, for reference) | +66,651 | about 0 (already linked) |
| Engine Read-root paths now reached: `SelectedRead`, caller root, `ReadCompletion`, `ReadRangeHistory` (estimate) | +8K to +20K | about 0 (OKX, WEEX and Bybit already instantiate them) |
| Binance venue code, net of the removed legacy funnel (estimate) | -2K to +5K | -2K to +5K |
| **Expected total** | **+6K to +25K, so about 1.24M to 1.26M** | **about 0 to +5K** |

The engine-path estimate is the uncertain part. Binance is a single-`Pending` family, so the generic read-root code instantiates once. The first source commit measures it, with the native `LegacyAuthority` history removed, before the PR run.

## Original REDs (next, before source)

A new target, `venues/binance/tests/history_context.rs`, run against the unchanged base with zero ignored:

1. **Managed fills page.** A fills page is a managed Read preparation with exact URL, query and read-key headers. Today it is a legacy signed request.
2. **Lookup.** The lookup is a managed Primary(`ClientOrderLookup`) preparation.
3. **Unanswered page.** An unanswered page ends at 20 s as a typed Timeout. A late reply publishes nothing.
4. **Root expiry.** Root expiry mid-walk is a strict Timeout with no rows and no marker.
5. **Unstamped command.** An unstamped command is refused NotSent before any I/O.

GREEN native controls, from `history_owned.rs`, `lookup.rs` and `queries_variants.rs`, are kept and re-pinned:

- window URLs and the order of fills;
- accepted prefix plus marker;
- income identity, sort and the 2 s chunk timer;
- keyless or symbol-less `[]` with no I/O;
- unsigned public trades;
- the `-2013`-only `ABSENT`;
- Spot and COIN-M unsupported results with no I/O.

Scoped Binance tests and strict checks only, with `CARGO_BUILD_JOBS=4`.
