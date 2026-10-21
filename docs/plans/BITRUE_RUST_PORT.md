# Bitrue futures Rust port

Branch: `feat/rust-bitrue`, based on `origin/feat/rust-rewrite`.
Spec: `RUST_REWRITE.md`; process and gates: `VENUE_PORT_BRIEF.md`.

## Plan and audit

1. Public data: typed streamed contract metadata, ticker seed/stubs and channel
   lookup, full snapshot depth (simple-depth then timed fallback), candles and
   websocket history requests, server-ping responses and reconnect watchdog.
2. Private data: exact timestamp/method/path/body HMAC, real futures URLs,
   listen-key create/refresh, bounded safe read retry, account/order reconciliation,
   one-way settings and stale-snapshot guards. Running totals yield individual
   executions with duplicate/regressive suppression and exact large IDs.
3. Trading: whole-batch preflight, caller IDs verbatim on one regular order,
   integral quantity splitting, quote-sized opening market orders, atomic attached
   TP/SL, standalone conditional protection, cancel/replace and settings commands.
   Only an explicit signature rejection (-1022) authorizes one re-signed resend;
   ambiguous writes never retry. Margin-mode changes and hedged mode fail closed.
4. Queries: documented per-execution account trades with exact IDs and window
   filtering; enumerate official endpoints before declaring a query unsupported.
   Avoid synthetic fills, PnL or ledgers inferred from unrelated fields.
5. Harness gates: WASM/SDK registration, generic replay extension, documented
   private tape with signatures and mutation matching, live 20-second/two-book
   public fixture, Binance regression, native/wasm quality gates and CI.

Each phase ends with a sub-agent audit, fixes, verification and a commit.
Keep transport routing host-side; this venue emits actual Bitrue HTTPS/WSS URLs.
Use borrowed serde types and typed continuations; no JSON value trees or JS facades.
Check crypto leftovers for provably unused code. Preserve existing venue gates.

## Landing

After all gates: merge latest landing, push, PR into `feat/rust-rewrite`, post
local adversarial sub-agent findings, fix/repeat, wait for green CI, merge PR,
create a clean follow-up branch. Never stash, rebase, force-push or touch master.

## Progress

- Plan committed; source/API audit in progress. Official documentation:
  https://www.bitrue.com/api_docs_includes_file/futures/index.html and
  https://github.com/Bitrue-exchange/USDT-M-Future-open-api-docs/tree/main/v2 .

### Public checkpoint

- Implemented typed public state and registered the venue with the WASM/SDK and
  replay harness; private/trading/query module groundwork compiles alongside it.
- Sub-agent public review PASS after fixing candle routing on reconnect,
  per-channel request serialization/correlation, timeout isolation, error replies,
  and symbol aliases. Five independent review regressions pass.
- Live 20-second tape: 776 markets/tickers, two full books, 3,287 replay frames.
  Gate PASS: TS/Rust CPU 243.8/151.7 ms; steady 131.6/89.3 ms;
  retained 2.9/2.4 MB. Fixture committed.
- Native public tests (8) and review tests (5) pass; private/trading/query phase
  audits and the documented private gate are still in progress.
- TS correction discovered from official payload units: apply contract size to
  public depth/ticker/candle volumes and retain contractSize metadata. Private
  units, lifecycle and cancellation corrections are being validated separately.

### Private, trading and query checkpoints

- Independent private review PASS: exact large IDs; stable native-contract fill
  conversion with individual L prices and cumulative-average fallback; bounded
  high-water/terminal guards; real futures listen key, 30-minute renewal and
  watchdog; atomic account/settings and paced per-symbol order snapshots.
- Independent trading review PASS after fixing protection erasure through an
  empty childOrders list. Batch preflight, quantity conservation, quote-market
  sizing, atomic brackets, safe cancel/detail/replace, caller-ID isolation and
  no uncertain write retry have direct native tests. Protective commands return
  the TS refresh result; the actual native state comes from reconciliation.
- Independent query review PASS after fixing side-specific fill ownership and
  rejecting incomplete filtered full pages. Official v2 has a forward fromId
  cursor with a most-recent default, no documented backward cursor, complete
  order-history list or complete PnL/funding ledger. These cases fail explicitly.
- Paired TS fixes also cover conditional/attached REST children, exact native
  cancellation identity, SL Mark / TP Last defaults, and malformed execution
  IDs / failed query propagation. KNOWN_TS_FIXES documents every divergence.
- Native venue tests pass (52); SDK WASM boundary test passes, proving real
  registration, worker/main frame identities/units, actual read-only leverage,
  recvWindow forwarding and exactly one newly signed -1022 resend.
- Harness adversarial fixes preserve create-ID field presence and caller IDs,
  require exact cancellation identity and signed private requests. 77 harness
  tests pass. Crypto exports remain used by this and existing venue crates;
  no further first-port leftover became unused.

### Harness review and final verification

- Independent harness review PASS after retaining validation failures in the
  signature counter even if a later signed GET retry succeeds; regression added.
  78 harness tests pass. The private tape requires all writes to be consumed,
  exact caller/native IDs and request bodies, signed private reads/writes,
  subscriptions and server ping replies, and all individual execution prices.
- Four required replay gates PASS after landing integration: Bitrue public
  (three runs), Bitrue private (20 frames), Binance public and Binance private.
  Bitrue three-run medians: TS/Rust CPU 293.0/224.0 ms; steady 187.2/141.2 ms;
  retained 3.7/2.4 MB. Binance CPU 284.6/149.5 ms, memory 4.2/2.3 MB.
- Final private price recovery also ties cumulative value to its exact filled
  quantity across REST advancement, including an earlier L-only fill. A new
  native and paired TS regression cover the interleaving.
- Typecheck (44 tasks) and lint (28 tasks) pass. Workspace native clippy passes;
  full workspace tests and final WASM rebuild are running. The default root
  test command runs both test-ci and test-unit concurrently; local unrelated
  5-second import timeouts are being rechecked after compilation settles.
- No budget increase: inherited landing budget is 3,145,728 bytes. Final head
  must pass every CI check before merge; no outstanding review waiver.

### Final artifact checkpoint

- Final required gates PASS: Bitrue public CPU 252.5/196.1 ms, steady
  164.5/121.2 ms, retained 3.7/2.4 MB; 3,287 frames/776 markets/two books.
  Private 20 frames PASS. Binance public CPU 258.5/130.0 ms, retained 4.2/2.3
  MB; public and 14-frame private gates PASS.
- Final WASM 3,138,885 bytes, below inherited 3,145,728-byte budget.
- Workspace tests PASS (641 including doctests), Bitrue tests PASS (53),
  complete SDK WASM tests PASS (22), host tests PASS (25), harness PASS (78),
  targeted Bitrue TS PASS (56). Native/wasm clippy, fmt and bans PASS.
- Final package typecheck and lint PASS. Updated older history tests to use
  documented native trade IDs and to expect rejected reads to propagate; no
  production exceptions were added for outdated test fixtures.
- Opening a draft PR for required local adversarial review and CI while the
  complete TS suite's local 30-second import-timeout recheck finishes. It must
  be ready and every CI check green before merging.

### PR #1314 adversarial round 1

- Posted three concrete P2 findings in PR comment 6275171502; fixed all three
  with regressions. Independent focused re-reviews PASS: Closed dominance and
  timestamp/amount watermarks survive empty reconciliation; malformed candle
  timestamps reject the complete history before filtering.
- Native Bitrue tests now 56 PASS, native/wasm clippy PASS and private gate PASS.
  Complete safe-cex suite PASS: 6,597 tests (30-second local import timeout),
  plus all 22 SDK WASM and 25 host tests PASS.
- Initial CI passed native/wasm/size and earlier venue gates, then rejected an
  invalid Bybit TS memory baseline (-136 backing-buffer bytes). Guards remain
  unchanged; latest-head CI will rerun.
- A final local public run narrowly missed steady CPU (Rust151.7/TS147.0ms),
  despite main CPU and memory passing. Optimizing duplicate ticker JSON scans
  before advancing the PR; no threshold or output normalization changes.

### Bitget landing integration and PR round 2

- Merged landing 3a7b46407d, retaining its structure, throughput profiles,
  complete-document guards and unified createTape transport. Independent
  integration review PASS: exact bodies, native/caller IDs, signature audit,
  subscriptions and worker-method dispatch retained. Harness 118 PASS.
- Round 1 findings independently re-reviewed PASS. Public single-pass decoder
  review found missing candle close could become zero; explicit presence and
  null rejection now have a regression and independent re-review PASS.
- Final integrated public gate (three-run medians): 3,287 frames, 776 markets,
  two books; TS/Rust CPU108.7/82.8ms, steady64.1/41.6ms, retained3.9/2.3MB.
  Bitrue private20 frames PASS. Binance public873 frames CPU188.1/121.3ms,
  steady98.7/59.5ms, retained4.4/2.3MB; private14 frames PASS.
- Native workspace724 tests PASS (Bitrue57); SDK WASM26 PASS, host25 PASS.
  Workspace native clippy, WASM host clippy and fmt PASS.
- The eleven-venue artifact is3,759,020bytes,262KB above landing's ten-venue
  artifact. Fixed combined budget becomes3.75MiB(3,932,160bytes), following
  the landing's aggregate per-venue budgeting. Bitrue uses size profile z;
  no correctness, CPU or memory threshold changed.
- Earlier full safe-cex suite6,597 PASS and paired Bitrue56 PASS remain valid;
  integration typecheck/lint and latest-head CI must pass before merge.

### Hyperliquid landing race and feature integration

- All checks on82591dfa92 were GREEN, including all25 replay gates and both
  repository CI jobs. CI Bitrue public CPU165.3/136.8ms, steady95.8/74.6ms,
  retained3.9/2.3MB; private20 frames PASS. Merge was rejected because
  Hyperliquid landed concurrently. No bypass or auto-merge used.
- Merged landing5c47683969. Its per-venue WASM features and matrix supersede
  the prior aggregate budget. Bitrue joins selectable/default features,
  build --venue selection and the CI matrix, retaining the existing fixed
  1,258,291-byte venue budget without increase.
- Retained landing's authoritative request/Tape and memory accounting. Restored
  route-scoped generated IDs, exact native/caller body IDs and persistent
  validator failures. Bitrue callback-ID normalization is venue-scoped;
  channel, operation and heartbeat audits remain enforced. Harness153 PASS.
- New landing Query::OrderByClientId explicitly rejects for Bitrue, matching
  other non-Hyperliquid venues, with regression proving no HTTP request.
- Both independent conflict-resolution reviews PASS, plus focused new-query
  review PASS. Typecheck44/lint28 tasks PASS. Native, combined SDK boundary,
  standalone artifact and public/private gates rerun before the next push.

### Standalone artifact final checkpoint

- Verification caught and fixed lost Bitrue recvWindow forwarding and two
  outer host feature gates. Existing SDK regression proves header12345;
  Bitrue-only clippy/build prove the constructor exists. Re-review audited
  every host cfg and PASS. All findings will be posted on PR1314.
- Workspace792 tests (Bitrue58), SDK WASM26, host25, harness153 PASS;
  fmt/bans, native/wasm clippy and Bitrue-only clippy PASS. Typecheck44 and
  lint28 tasks PASS, plus package reruns after recvWindow fix PASS.
- Final Bitrue-only artifact709,861bytes < unchanged1,258,291-byte budget.
  Public3,287 frames/776 markets/two books, three-run TS/Rust medians:
  CPU255.4/188.5ms, steady153.5/115.8ms, retained3.7/2.4MB PASS.
  Private20 frames PASS. Binance public873 frames CPU261.4/115.3ms,
  steady147.2/49.3ms, retained4.2/2.2MB PASS; private14 frames PASS.
- Push this reviewed integrated head for all matrix/repository CI checks;
  merge only green, into feat/rust-rewrite, then create fresh follow-up.

### Resume checkpoint: all gates green, latest roster merged

- Head583104665d passed every CI check:12 venue jobs (27 public/private
  replay gates), all native/WASM/static jobs, both repository CI test jobs,
  secret scans. CI Bitrue artifact709,935/1,258,291bytes; CPU189.5/168.4ms,
  steady107.1/83.5ms, retained3.7/2.3MB; public3,287/private20 frames PASS.
- On explicit resume instruction, fetched and merged landingc5bc2c4adf first.
  Independent exact-head review PASS: only four roster lines in RUST_REWRITE
  changed, with runtime/transport/build/CI identical to the tested head.
- Progress and prior adversarial findings are committed and posted on PR1314.
  Push this docs-only merge checkpoint and wait for its CI before merging;
  destination remains feat/rust-rewrite. Create fresh follow-up afterwards.

### Port-review lows plan (Codex, 2026-10-21)

1. Decode private order conditionOrder/triggerType and classify SL/TP before
   the generic execution MARKET/LIMIT type. Keep cached protective child
   direction/identity, price/reference, reduce-only and fill accounting unchanged.
2. Reproduce uncached stream misclassification; cover numeric/string trigger
   values, false conditional flags, cached children and interval regressions.
3. Strike stale catalogue/order-loaded findings; retain malformed account
   snapshot admission as intentional/RISKY to bypass, with usable state preserved.
4. Run focused Bitrue/Binance tests, touched fmt/strict Clippy/bans, independent
   review and exact-head CI; merge only after the overseer's short hold lifts.

- Reproduced uncached conditional MARKET decoded as Market instead of StopLoss.
  Private OrderEvent now decodes conditionOrder and numeric/string triggerType;
  explicit native SL/TP wins over execution type. Cached protective child kind,
  side, parent and reduce-only provenance remain authoritative. Native positive
  trigger price/recognized reference still precede their cached fallbacks.
  No price-based guessing, quantity scaling or execution accounting changes.
- Catalogue and per-symbol order startup findings are stale (existing row
  isolation and first-attempt sweep completion). Malformed account admission
  remains an explicit risk to bypass; prior balances/exposure remain intact.
  Native/TS discriminator schema: https://www.bitrue.com/api_docs_includes_file/futures/index.html
  The live docs define conditionOrder and triggerType1/2 in order records; the
  TS private stream uses those exact fields and precedence.
- Validation: the new classification regression fails before repair;377 focused
  Bitrue/Binance tests pass, then all35 account tests pass with the extended cached
  child precedence case. Touched formatting, strict all-target Clippy and bans pass.
  Independent read-only review found no runtime blocker; corrected its docs note
  that cached price/reference are fallbacks behind valid native trigger fields.
