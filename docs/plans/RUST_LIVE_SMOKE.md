# Credentialed lifecycle smoke

Branch: `feat/rust-live-smoke`, base `origin/feat/rust-rewrite`.

No credentialed network run is authorized in this task. Verification uses
scripted transports only. The executable uses the existing Wasm EngineHost;
native network primitives provide its transport. No engine shapes change.

## Phases

1. Configuration and private recording: explicit environment selection,
   per-venue environment-only credentials, endpoint fences, canonical private
   tapes with credential/session/signature redaction. Document supported engine
   test environments and credential names in RUST_REWRITE.md. Review and commit.
2. Lifecycle: wait for private readiness, inspect the account frame, select an
   active market, derive a minimum-quantity resting buy from current book and
   market constraints, place once, amend, cancel, cancel symbol orders and
   restore the original leverage. Bound waits, never retry writes, preserve
   partial native IDs, attempt cleanup on failure. Review and commit.
3. Gates: scripted fake transport drives the real Wasm host, records the run,
   and replays its tape through TS/Rust. Exercise configuration, secret leaks,
   sizing, failed/ambiguous writes and cleanup. Run existing harness and Binance
   regression gates and repository checks. Review and commit.
4. Landing: merge latest landing branch, push PR into feat/rust-rewrite, post
   independent adversarial correctness findings, repair and re-review, merge
   only at a green exact CI head, cut a fresh follow-up branch.

## Safety and compatibility audit

- No default production fallback: choose a reviewed test/demo profile or refuse.
  Mainnet requires the explicit --mainnet switch. Every REST/socket destination
  is checked against the profile before transport I/O; redirects are refused.
- No credential arguments, files or arbitrary endpoint overrides. No raw engine
  log, exception or request reaches stdout/stderr. Private tapes retain account
  and order data but no usable authentication material; create with mode 0600.
- Use authoritative current book quotes, a tick-aligned price below the bid,
  and the smallest step-aligned amount permitted by discovered size/notional
  rules. Never increase size after rejection. Refuse unknown leverage, invalid
  books, non-empty account orders/positions and insufficient sizing metadata.
- A dedicated empty account is required because cancel-all is intentional.
  Cancel the selected symbol to bound scope. Never flatten an accidental fill.
- Amendment may replace the native ID: retain both original and returned IDs.
  Ambiguous placement triggers symbol cleanup but no placement retry.
- Live private recordings gate correctness, not performance. Existing public
  fixtures remain the CPU/memory regression evidence. Unsupported venue features
  fail explicitly; do not silently mark skipped lifecycle steps passing.

## Progress

- Read landing brief, rewrite spec, root/package exchange contracts; clean fresh
  branch created from latest landing. Configuration and recorder work delegated
  in disjoint scopes. Lifecycle design under implementation.
- Phase 1 profiles: `7f3b4b4579`, independent review PASS, 11 tests. Repaired
  Bybit demo mode and added simulation payload/header fences for shared origins.
  Recorder review additionally required positional auth redaction and coherent
  typed replay credentials/session identities; repairs under final re-review.
- Real Wasm host + scripted Binance transport completes the lifecycle and its
  recorded tape passes the actual TS/Rust private gate. Lost placement reply,
  failed replacement, and a nonempty initial account pass negative tests. No
  real account or credentialed network request was used.
- Cancel-all uses the SDK generic strategy: cancel each observed account symbol
  plus the selected symbol. Direct `CancelSymbolOrders {symbol:""}` is invalid
  on Binance and several other engines; native-global venues accept symbol
  commands through their implementation. Initial account emptiness bounds scope.
- Live tapes carry an explicit completion result. The gate refuses failed or
  incomplete lifecycle captures before replay. Binance LIST_SUBSCRIPTIONS is
  normalized as a liveness probe, like ping/pong; feed checks remain exact.
- Phase 1 recorder: `a3f7eef4b4`, independent re-review PASS, 106 recorder/Tape
  tests. Outgoing authentication is omitted; only protocol request correlation
  records persist. Canonical real environment URLs, scoped identity aliases,
  interrupted-startup bootstrap and recorder sealing preserve replay semantics.
- Phase 2 source review PASS after fixing late final-flush fills and optimistic
  cleanup proof. A fresh engine reads remote account state after disposal;
  namespaced verification records remain diagnostic, while the primary lifecycle
  is compared through TS/Rust. Unknown notional evidence refuses before writes.
  After cancellation the runner tests a lower leverage where available, then
  restores the captured original; fixed/minimum leverage remains unchanged.
- Audited sizing extension: `e65f363a33`, 21 source-linked venue families and
  33 passing tests, independent review PASS. Native place/amend units override
  display-unit min/step/max for KuCoin, BloFin, OKX and MEXC. Unknown monetary
  rules refuse before trading; current limitations are listed in the spec.
- Security re-review: `38ccb04020`, signed auth-header echoes and canonical form
  encodings now redact, with reproduced failures before repair and 109 passing
  recorder/Tape tests afterward.
- TS bug found by the actual lower/restore gate: Binance one-way placeholders
  use the bare symbol, while setLeverage updated only legacy hedge IDs. The
  stale value caused restore to skip its write. Update actual matching IDs and
  propagate rejected writes; 3 native TS regressions and independent review
  pass. KNOWN_TS_FIXES documents the fixed reference without output rewriting.
- Merged landing `5d7589f296` (GRVT and memory-baseline changes) as `88b5c3c805`.
  GRVT has no reviewed capture/replay profile yet and refuses before credentials.
- Final local gates: 471 harness cases; 9 real-Wasm fake lifecycle scenarios
  including an actual captured-tape TS/Rust gate; 3 TS leverage regressions;
  103 Wasm SDK cases, 65 JS host cases, 38 native host cases and 29 native
  Binance cases. Bans, workspace fmt, native/wasm clippy pass. Root 44 typecheck,
  28 lint and 27 test tasks pass after the TS repair.
- Binance public gate, 3 runs/873 frames/917 markets/783 tickers/3 books: TS/Rust
  CPU 289.4/161.3 ms, steady CPU 165.6/79.4 ms, retained memory 16.3/3.2 MB.
  Existing Binance private gate PASS (14 frames/2 markets) and captured fake
  lifecycle gate PASS. Private performance is informational. No real account
  was contacted; there is no credentialed live proof claim.
- All phase reviews pass after repairs. Next: final secret scan and landing
  refresh, PR into feat/rust-rewrite, posted adversarial reviews, exact-head green
  CI, normal merge and fresh follow-up branch.
- Final landing refresh also brought BreakoutProp/polling infrastructure as
  `c2cee66bf4`. Preserve its workspace/SDK/harness changes; the new venue has no
  reviewed live-smoke profile and explicitly refuses before credential access.
  Re-run shared harness, current default wasm build and the fake lifecycle gate.
- Post-Breakout build and replay PASS: 503 harness cases, 9 real-Wasm fake
  lifecycle scenarios, full default wasm build, 44 typecheck and 28 lint tasks.
  Incoming polling records remain opt-in and preserve Binance private pacing.
  Independent merge review PASS. Root 27 test tasks passed immediately before
  this incoming merge; final PR CI runs the full combined state.
- Opened [PR #1332](https://github.com/Tealstreet/tealstreet-sdk/pull/1332) into
  feat/rust-rewrite. Posted three adversarial reviewers' findings at exact head
  `6de2cc4470`: default Bitget simulation quote selection, short credential
  echoes, and delayed reads exposed before their command stage.
- Round 2 repairs: `0220a38471` plus selector/profile guards. Sensitive short
  returned auth refuses recording; failures latch across headers, URLs, bodies
  and decoding. Environment credentials require at least four characters while
  public numeric indices remain valid. Reads fence at full response completion;
  mutations retain their issuing stage. SUSDT/SUSDC are selectable by default.
  Independent repair re-review PASS, 117 recorder/Tape cases and all 9 real-Wasm
  fake scenarios pass. Combined harness now 513 passing cases.
- Latest post-Breakout Binance public gate (3 runs): TS/Rust CPU 317.7/176.2 ms,
  steady CPU 176.2/84.4 ms, retained memory 16.4/3.3 MB. Existing private gate
  PASS. Bans/fmt/current wasm clippy and secret scan pass; no size budget change.
  Pending only pushing reviewed repairs, posting their passing re-review, and
  exact-head green CI before normal merge/fresh branch.
- Exact repaired head `e4ffa9d48f` reached 35/35 green CI, but final merge guard
  detected the newly landed Binance variants PR #1326. Merge landing
  `e391337054` preserving its nested workspace, all variant scripts and harness
  protocol checks. New binancepm/inverse/spot smoke profiles explicitly refuse
  every mode before credential access; USD-M binance stays enabled.
- Integration review caught the incoming Binance validator rejecting signed
  demo listen-key traffic, although both engines sign it. Regression reproduced
  before repair; exact demo-fapi/demo-dapi origins now use full HMAC/time/window
  validation, while production listen-key stays unsigned. Recorder unit replay
  now supplies genuine fresh replay signatures, preserving the new oracle.
  Full harness 533/533 and all 9 real-Wasm fake lifecycle scenarios pass. A
  second independent merge review passed 193 scoped units plus fake lifecycle
  replay; no remaining concrete integration finding. No real accounts used.
- Combined landing gates PASS: 533 harness, 9 scripted real-Wasm lifecycle
  scenarios, 108 Wasm SDK and 99 native Binance tests; root 44 typecheck /
  28 lint / 27 test tasks. Bans, fmt, native Binance and wasm clippy, full
  branch secret scan pass. Binance private gate PASS (14 frames/2 markets).
  Public gate 3 runs/873 frames/917 markets/783 tickers/3 books: TS/Rust CPU
  298.6/177.5 ms, steady CPU 175.2/96.3 ms, retained memory 16.3/3.3 MB.
  Merge review PASS; post the repaired-head evidence and wait for its full CI
  before merging PR #1332, then cut the fresh follow-up branch.


## Hyperliquid Spot beta smoke

Hyperliquid and Hyperliquid Spot share the HYPERLIQUID environment fields:
SIGNER_ADDRESS is the agent (API) wallet address, PRIVATE_KEY is that agent's
key, and ACCOUNT_ADDRESS is the funded account. Never provide the main wallet
key. The mainnet beta run must explicitly use --mainnet and a dedicated account
with no open orders; use --symbol UBTC/USDC for Spot. Spot leverage is fixed at 1,
so the harness never attempts a leverage change. Existing write caps, no-retry,
post-only and fresh cleanup readback checks stay in effect.

Spot sizing requires a successful spotMetaAndAssetCtxs reply that uniquely
identifies the selected pair, its base lot precision and USDC quote token.
The documented MinTradeSpotNtl floor is 10 quote tokens, distinct from the
perpetual dollar floor: [official error contract](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/error-responses).
Non-USDC quotes and missing or inconsistent metadata refuse before placement.
Profile and scripted checks do not claim a successful credentialed live run.
