# ApeX Omni native port

Branch: `feat/rust-apexomni`, based on `origin/feat/rust-rewrite`.
Scope: ApeX Omni USDT/USDC perpetuals (`apexomni`); no spot, deposits,
withdrawals, account registration, wallet/provider adapters, or RWA context.

The port follows `VENUE_PORT_BRIEF.md` and `RUST_REWRITE.md`. Each phase has
a plan audit, implementation, focused verification, independent sub-agent
code-correctness review, fixes, and a stable commit. Full auto is authorized.

## Phases

1. Public data: streamed contract config, zero ticker seeds then live tickers,
   200-level snapshot/delta books, candles and public trades, socket roles,
   reconnect subscriptions and liveness. Review before committing.
2. Private data and authentication: exact API HMAC and compatibility retry,
   account identity and fee config, orders/virtual brackets, positions,
   settings and balances, private socket login/subscriptions/heartbeat,
   bounded fill cache, atomic snapshots and stale-response guards. Review.
3. Trading and native zkLink signing: serialize the exact perpetual contract
   signing payload, verify SDK oracle vectors, validate caller IDs before
   signing/writes, FIFO placement/brackets/amend/cancel/settings jobs. Review.
4. Queries: typed streamed execution history with symbol candidates,
   history-orders fallback and private-fill cache; client-ID lookup,
   explicit unsupported capabilities where TS has no venue implementation.
   Review.
5. Integration and gates: WASM feature/SDK registration, independent request
   and signing audits in private tape, live public tape and committed fixture,
   strict comparison/performance/size gates, Binance regressions, workspace
   checks, roster update. Review, merge latest landing branch, rerun affected
   gates, push and open PR into `feat/rust-rewrite`.
6. PR code-correctness reviews: post findings, fix and repeat, require green
   CI, merge into `feat/rust-rewrite`, cut a clean follow-up branch.

## Signing design and audit requirements

API authentication uses the UTF-8 bytes of the standard padded base64 of
the secret as its HMAC-SHA256 key; the digest is standard padded base64.
The primary preimage is timestamp-ms + uppercase method + `/api` + path,
with sorted percent-encoded query parameters for reads, or exact sorted
form bytes for writes. Meaningful false/zero values survive primary signing.
Klines carry no authentication. The private socket signs timestamp-ms +
`GET/ws/accounts`, sends login before subscribe, and does not await a venue
acknowledgement. Real direct URLs and explicit socket roles leave routing
to the host.

Exactly one confirmed signature rejection may resend with TS's legacy
serializer: second-truncated timestamp, falsy filtering and route-specific
legacy body construction. Transport failure may retry reads only. Native
typed request descriptors retain the original semantic fields and encoding
so a resend never signs a previously mutated request by accident. Tests pin
primary/legacy preimages, final transmitted bytes and retry bounds.

L2 order signing must use native Rescue/MuSig, never the JS/WASM SDK at
runtime. Audit `safe-cex-crypto::zklink` against the bundled npm/native SDK
vectors and inspect the pinned SDK contract serializer before implementing
venue packing. Pin seed decoding/derivation, transaction discriminator,
field widths/endian order, packed size, price, pair, direction, maker/taker
fee rates and subsidy bit with independent vectors. Reject invalid seeds,
overflow/unrepresentable values and malformed account/contract config
before any trading write.

For each parent and attached TP/SL leg, SHA256 the UTF-8 client ID; reduce
account ID and nonce modulo `2^32-1`, derive slot from digest modulo
`2^64-1` divided by `2^32-1` using the SDK's exact integer conversion.
Decimal size/price scale by `10^18` with truncation toward zero. Fee rates
are ceil(rate*10000), maker magnitude is absolute, subsidy is explicitly
false. Account ID comes from `/v3/account`, pair ID from symbols config.
REST expiration is now+30 days rounded up to epoch hours plus 168 hours;
it is separate from the contract signature. Limit fees round upward to six
decimal places. Confirm every detail against TS and SDK oracle, including
boundaries and negative maker fees.

The perpetual contract serializer is a 36-byte SDK payload: discriminator
`0xfe`, account u32 BE, subaccount u8, low 16 slot bits BE, low 24 nonce bits
BE, low 8 pair bits, direction u8, packed40 size, u120 BE price, maker fee
magnitude u8, taker fee u8, subsidy u8. SDK oracle vectors must pin each
truncation and the packed40 exponent/mantissa, rather than assuming the
builder's wider types survive serialization.

Caller client IDs are non-empty without whitespace, honored verbatim as
both `clientId` and `clientOrderId` for one plain native Limit/Market order.
Reject duplicate IDs, splitting, triggers or attached brackets before any
batch write. The active exchange-local generator uses
`apexomni-<account>-<milliseconds>-<six-digit-random>`; the unused utility
generator has a different prefix and is not the placement contract.
Split brackets must fail before signing/writes because no documented atomic
multi-parent bracket contract exists. Amend is cancel+replace with a fresh
generated ID and preserves both virtual siblings when editing one leg.

## Verification and progress

- Initial repository/spec/TS inventory underway; no credentials required for
  deterministic documented-payload private tapes. Live authenticated testing
  requires a separately available account and is not claimed by replay.
- Required gates: bans, fmt, all-target clippy, native tests, WASM clippy,
  private tape/gate, 20-second/two-book public recording/gate, Binance public
  and private regressions, root typecheck/lint/tests, green PR CI.
- Gate results, review findings, documented TS fixes, PR and merge commit
  will be recorded here as phases complete.

### Public phase

- Streamed contract config; canonical markets and native signing config;
  zero ticker seeds, live tickers, full book snapshots/deltas, subscriptions,
  candles/trade updates, real URLs, public role, watchdog/reconnect.
- Focused tests: 11 passing. Independent code-correctness review found
  rounded quote-volume division and nonempty book clearing notification;
  both fixed with regressions, bounded re-review passing.
- Documented venue depth contract requires continuous `data.u`; Rust waits
  for snapshots and resubscribes on gaps. TS reference correction pending.

### Signing and trading verification in progress

- Detailed byte-level design/evidence: `RUST_APEXOMNI_SIGNING.md`.
- 12 signer tests passing, including three fresh signatures captured from
  unmodified `zklink-web-sdk-alpha@1.0.2`, Unicode IDs, packed-size boundary,
  negative maker fee and narrowed pair/slot/nonce fields.
- First 11 trading tests passing: caller IDs/batch preflight, split brackets,
  fee precision, trigger rounding, independent bracket signatures, FIFO,
  no ambiguous write retry, bounded signature rejection retry, cancellation,
  and valid multipart leverage requests.
- Trading review found decimal limit-fee rounding and trigger rounding
  order; fixed. Exact whole-tick splitting and preserving both siblings on
  ordinary amendments additionally repair TS quantity/protection bugs.
- Account phase: 18 integration tests and one bounded-guard unit test passing.
  Independent code-correctness review passed after silent handshake watchdog,
  missing-passphrase preflight, bounded revision/tombstone fencing, genuine
  execution-only notifications and acknowledged-leverage snapshot guards.
- Queries: 11 focused tests passing; independent code-correctness review
  passed. Native execution identity preserves distinct equal fills; TS oracle
  correction is in progress. Unsupported income/order history stays branded
  unsupported in the SDK; public-trades history fails explicitly.
- Trading bounded re-review passed after decimal limit-fee and trigger-price
  corrections. Native clippy (all targets, warnings denied) passes.
- Initial single-venue WASM build: 954,742 bytes, below 1,258,291-byte cap.
  Final gates await paired TS oracle fixes and strict replay integration.
- Confirmed additional TS auth defect: resend saw transformed cancel strings
  or platform-specific multipart internals. Retaining original semantic payload
  makes legacy signing deterministic. Paired TS/API regressions in progress.

### Paired reference and harness phase

- Paired TS financial corrections pass independent code-correctness review:
  strict failed-read guards, canonical hedged IDs/mode, realized-PnL alias,
  native execution identity and raw millisecond anonymous identity, synthetic
  cache isolation, per-execution notifications, exact tick splits and sibling
  preservation. SDK wrapper forwards Omni seed/auth and keeps unsupported
  history/income capabilities branded. Account identity is a typed engine read.
- Public live tape: 20 seconds, 245 received frames, two books. Committed
  six-second gzip fixture: 53 records / 51 frames, 89 markets/tickers.
- Public gate PASS: main CPU TS 68.0 ms / Rust 29.8 ms; steady CPU 42.6 /
  3.9 ms; process CPU 77.7 / 66.5 ms; retained memory 3.4 / 2.0 MB; Rust
  linear memory 1.6 MB. Identical output. These are initial branch numbers;
  final acceptance repeats after landing integration.
- Live traffic exposed a TS pong-ack bug: nested `request.op=ping` was mistaken
  for a server ping. Top-level control parsing fixes it; regression/review pass.
- Strict private harness independently verifies API HMAC and unmodified npm
  zkLink signatures for every parent/TP/SL leg, sizes, fee/expiry, broker,
  caller IDs, socket role/auth/subscriptions/generations and ping responses.
  Mutation tests cover mismatched multipart boundaries, invalid fallback pongs,
  broker-prefixed caller IDs and legacy hour-boundary expiry quantization.
- 177 harness tests pass. Root typecheck 44 tasks and lint 28 tasks pass.
- Review additionally required unique generated IDs within precomputed batches:
  per-job reservation/retry prevents six-digit random suffix collisions and
  reserves caller/known IDs. Deterministic collision regression added.
- Next: commit paired corrections/integration, merge latest landing branch
  (Orderly and Lighter), rerun all native/WASM/root checks, public/private and
  Binance gates, then PR code-correctness reviews and green CI.

### Landing integration

- Paired phase committed at `d2ffb5155b`; 75 focused ApeX TS tests and 177
  harness tests pass. All reported phase review findings are fixed/re-reviewed.
- Integrated landing `f66bdc26f9` (Orderly and Lighter), retaining its nested
  workspace, all venue aliases/features/transport and SDK loader changes.
  Merge conflicts were resolved by disjoint file owners and reviewed against
  the landing version. Merged harness: 222 tests passing.
- Final gate run starts from this merged state. No thresholds, output
  normalization or memory accounting were weakened.

### Acceptance checkpoint before Bybit variants landing

- Combined SDK WASM tests: 52 passing; host tests: 42 passing. Workspace
  all-target clippy, bans and fmt pass. Strict private gate passes: 42 frames,
  18 commands, 21 requests, 12 write attempts and 9 subscriptions in each
  implementation; zero invalid signatures/protocol errors/missing writes.
- Root lint passes 28 tasks. Full repository tests pass 26/27 tasks; only
  two unchanged HyroTrader cold-import tests exceed their five-second timeout.
  Isolated HyroTrader suite passes 10/10. A bounded-worker full SDK run is
  underway without changing assertions or timeouts.
- Landing advanced to `922b5fdc89` (Bybit variants), which includes the
  configuration-test cold-import repair. Commit this checkpoint, integrate
  landing, then repeat affected acceptance gates on the merged state.

### Bybit variants integration

- Landing `922b5fdc89` integrated at `dd154bcfda`. Independent reviews verify
  all landing engine/host, Bybit variants, wrapper and gate contracts remain;
  ApeX's `FetchAccount`, auth, strict protocol oracle and paired TS fixes remain.
- Adapted replacement orders to the engine's new Bybit-only fields. ApeX
  rejects either field before any batch I/O or mutation. Focused trading
  tests: 12 passing; independent code-correctness review passes, including
  zero tick values, mixed-batch atomicity and queue recovery after rejection.
- Post-merge harness: 242/242 passing; root typecheck: 44/44 tasks; lint:
  28/28 tasks. Native, combined WASM, root tests and replay gates continue.

### Acceptance replay correction

- Root repository checks pass 27/27 tasks; each SDK test task passes 7,026
  tests. Combined SDK WASM tests pass 74 cases; host tests pass 45.
  Workspace native tests pass 1,347 cases before the three new regressions.
  Workspace all-target and default WASM clippy pass; secret scan finds no leaks.
- The final private replay caught guarded snapshot replacement iterating a
  hash map and reordering tied hedged sides. Replaced existing rows in place,
  append omitted guarded live rows in Store order, then derive placeholders;
  WS updates also preserve existing slots. No gate normalization changed.
- All 27 account tests pass, including three exact-order/stale-close cases.
  Independent code-correctness re-review, crate all-target clippy and default
  WASM clippy pass. Combined WASM rebuilt; strict replay is repeating.

### All local acceptance gates pass

- Final ApeX native suite: 81/81 tests. Bans and workspace formatting pass;
  crate/workspace all-target clippy and default WASM clippy pass. Root checks:
  typecheck 44 tasks, lint 28, tests 27; both SDK suites 7,026 cases. Harness
  242, combined WASM integration 74 and host 45 tests pass.
- Strict private replay: 95 tape records, 42 frames, 18 commands, 21 requests,
  12 write attempts, 9 subscriptions, 2 execution notifications in each engine.
  Signature/protocol/unserved/missing/unused-write counters all zero. Exact
  account positions, settings, fills and command output match after the fix.
- ApeX public fixture (three runs): 51 frames, 89 markets/tickers, two books.
  Main CPU TS 94.8 / Rust 43.3 ms; steady 60.4 / 11.0 ms; process 129.2 /
  95.1 ms; retained memory 23.7 / 2.2 MB; WASM linear 1.6 MB. Exact output.
  Node/web single-venue WASM: 974,848 bytes < unchanged 1,258,291-byte cap.
- Binance public regression (three runs): 873 frames, 917 markets, 783 tickers,
  three books. Main CPU 256.5 / 135.0 ms, steady 138.4 / 62.4 ms, retained
  memory 18.9 / 2.6 MB. Private regression: 14 frames, exact account/command
  parity. Both PASS; private performance remains informational.
- Logs: `/tmp/apex-orderfix-private.log`, `/tmp/apex-accepted-public.log`,
  `/tmp/apex-binance-public.log`, `/tmp/apex-binance-private.log`.
  Next: PR into `feat/rust-rewrite`, independent code-correctness reviews
  posted on that PR, address findings, green CI, merge and fresh branch.

### PR code-correctness review

- Opened [PR #1324](https://github.com/Tealstreet/tealstreet-sdk/pull/1324)
  into `feat/rust-rewrite` at `49998f94fb` after every local gate passed.
- Three fresh independent adversarial code-correctness reviewers cover native
  signing/trading, native public/account/history, and TS/harness/SDK-host
  integration. Findings will be posted on the PR and resolved/re-reviewed.
- CI is running; merge is authorized only after green checks and review.

### PR review round 1 findings

- Posted independent code-correctness findings on PR #1324: pre-start account
  identity regression (P2), malformed financial row identifiers committing
  empty snapshots (P2), subsecond history bounds losing fills (P2), and
  inherited price-only partial-fill amendment over-ordering (P1).
- Wrapper lifecycle correction passes 55 ApeX/Orderly tests, package typecheck
  and lint. Fresh identity lookup now starts lazily and awaits readiness;
  tests cover incomplete startup, failure and disposal.
- Paired native/TS identifier and raw-time fixes pass 86 native / 58 TS tests
  before amendment observer hooks. Final combined verification waits for the
  bounded partial-amendment fix and expanded private tape.

### Review fixes ready for landing integration

- Paired identifier/raw-time fixes, native/TS partial-amendment residual and
  observed-fill watermark guards, finite/no-op amend preflight, and API-only
  identity host creation are complete. Each fix passed an independent bounded
  code-correctness cross-review; the original malformed/time/mixed-fill
  reproductions now pass. Explicit replacement amounts retain API semantics.
- Focused checks: 23 native trading tests; 75 TS auth/public/parity/amend tests;
  57 ApeX/Orderly wrapper tests; package typecheck and lint pass. Real native
  identity before start, without an Omni seed, passes without streams/timers.
- Expanded private tape adds raw-millisecond REST and cache ranges plus a
  partial-fill price-only amendment. Final replay and size/performance checks
  will run after KuCoin landing integration (`373cc09864`, PR #1312).
- Replacement sizing observes events through the cancel acknowledgement and
  source timestamp watermarks. It does not claim atomic venue amendment or
  knowledge of unseen/later fills; observation storage is bounded.

### KuCoin landing integration

- Review fixes committed at `271bf7de80` after bounded cross-reviews passed.
- Integrated `373cc09864` (KuCoin #1312). Disjoint owners resolved SDK host
  lifecycle, WASM feature/options/dispatch, request normalization and build
  selection conflicts. Three-way audits preserve all 17 default venues,
  KuCoin hydration/broker contracts and every ApeX auth/identity/replay hook.
- 151 targeted shared harness tests pass during merge review. Full acceptance
  repeats from the merged state; private tape now includes review regressions.

### Post-KuCoin verification checkpoint

- All ApeX TS tests pass 113/113 across eight files. Two older bracket tests
  needed complete valid-order fixtures (`status`, `filled`, `remaining`);
  assertions are unchanged. Root typecheck 44 tasks and lint 28 tasks pass.
- Merged harness passes 264 tests. Combined 17-venue WASM rebuild passes.
  Expanded TS private replay passes 23 commands / 16 writes with all strict
  error counters zero; the partial order replaces 0.004 after 0.006 filled.
- Workspace native, paired private, combined WASM/host and final root tests
  are completing. Public performance/size and Binance regressions repeat
  after the current shared artifact is released.

### All reviewed gates pass before AsterDEX landing

- Reviewed state: 1,466 workspace native tests / 98 ApeX, 113 ApeX TS,
  264 harness, 75 combined WASM and 45 host tests pass. Root checks:
  typecheck 44 tasks, lint 28, tests 27; both SDK suites 7,065 tests.
  Bans, fmt, workspace/default-WASM clippy and secret scan pass.
- Expanded private gate PASS: 121 tape records, 50 frames, 23 commands,
  16 writes, strict signature/protocol/missing-write counters zero.
- Public gate (three runs) PASS: 51 frames/two books; main CPU 77.6 / 33.6
  ms, steady 46.7 / 4.4 ms, process 113.6 / 77.6 ms, retained 23.9 / 2.2
  MB. Single WASM node/web each 980,775 bytes < unchanged 1,258,291 cap.
- Binance public (three runs) PASS: 873 frames/three books; CPU 189.9 / 79.0
  ms, retained 18.9 / 2.6 MB. Private (one run): 14 frames, exact parity PASS.
- Landing advanced again to `d9aa597fd3` (AsterDEX #1322). Commit this
  stable gate checkpoint, integrate its shared account/read/route contracts,
  adapt the new STP field, then repeat affected checks before final PR push.

### AsterDEX landing integration

- Integrated `d9aa597fd3` (AsterDEX #1322), preserving its request routing,
  timeouts, currency-balance frames, shared account/book/STP commands, auth
  and all 18 venue features. Kept one shared `FetchAccount` variant and both
  account wire aliases. ApeX keeps API-only host initialization and signing.
- Three-way SDK/WASM/harness audits pass; 149 targeted shared harness tests
  pass. Lockfile/manifest union validates with locked cargo metadata. CI matrix
  includes both venues without raising size caps; roster removes both from
  the future-port list. Build selection supports both and existing aliases.
- ApeX explicitly rejects the newly modeled unsupported STP control before
  any batch I/O; replacement defaults it absent. Added all-mode single/mixed
  batch and account wire-alias regressions. Full post-merge gates follow.

### Final post-AsterDEX acceptance

- Source checkpoint `a12a57e207`: all 1,579 workspace native tests pass,
  including 99 ApeX tests. Combined 18-venue WASM build, 75 SDK WASM tests,
  51 host tests and 274 harness tests pass. Root typecheck passes 44 tasks,
  lint 28 and tests 27; both full SDK suites pass 7,097 tests each.
- Bans, workspace formatting, workspace all-target clippy, default WASM
  clippy and the full branch commit-range secret scan pass. All reported
  code-correctness findings are fixed and independently cross-reviewed;
  rounds 1 and 2 findings/resolutions are posted on PR #1324.
- Expanded paired private gate PASS: 50 frames, 23 commands, 16 writes;
  strict signature, protocol, unserved and missing-write counters are zero.
  This uses deterministic synthetic traffic, not a funded live account.
- ApeX public gate (three runs) PASS with identical output: 51 frames,
  89 markets/tickers and two books. TS/Rust main CPU 95.5/43.6 ms,
  steady 55.5/9.0 ms, process 135.8/95.8 ms, retained memory 23.9/2.2 MB;
  Rust linear memory 1.6 MB. Single-venue node/web WASM each 1,010,983
  bytes, below the unchanged 1,258,291-byte cap. One initial TS memory
  sampler deadline passed on an unchanged retry; no threshold was altered.
- Binance public (three runs) PASS: 873 frames, 917 markets, 783 tickers,
  three books; TS/Rust main CPU 191.7/89.1 ms, retained 19.0/2.6 MB.
  Binance private generator (30 records) and one-run gate PASS: 14 frames,
  two markets/tickers and no books, exact state/command parity.
- Final documentation checkpoint follows the reviewed source. Merge into
  `feat/rust-rewrite` remains conditional on green CI for that final head;
  after merging, cut `feat/rust-apexomni-followup` from the landing branch.

### Kraken landing integration

- Landing advanced to `7da56dc8a5` (Kraken #1323) during final CI. Integrate
  its venue registration, protocol hooks and roster row while preserving
  ApeX account lifecycle/signing and all 19 default venue features.
- Disjoint SDK, WASM and harness owners audit the three-way merge. ApeX
  native source and shared engine/crypto are unchanged; affected registration,
  combined builds, wrapper/harness checks and replay gates repeat afterward.
- Three-way merge reviews pass: all 19 features/dependencies/lock entries
  match, both venue descriptors and API-only identity behavior survive,
  and no signature or comparison thresholds change. Harness passes 286
  tests; SDK typecheck/lint and 33 focused wrapper tests pass. Final
  post-merge results are also posted on PR #1324 before green-CI merge.

### Port-review lows plan (Codex, 2026-10-21)

1. Preserve operand decimal precision when subtracting filled size from order
   amount, including a late fill during default-size amendment. Do not snap
   public remaining size to a market lot step or broaden fill accumulation work.
2. Isolate typed invalid zero-price REST candles with coded row warnings and
   retain sorted valid peers. Keep malformed timestamps/envelopes and a nonempty
   unusable result visibly failed; valid empty histories remain successful.
3. Record explicit client-ID rejection on multi-leg/protective work and verified
   leverage writes as retained risks to change; revalidate private route ownership.
4. Reproduce safe gaps, focused ApeX/Binance gates, touched fmt/strict Clippy/bans,
   independent review, caller/spec notes and exact-head CI; respect merge hold.

- Reproduced 0.3-0.2 exposing0.09999999999999998 remaining and one zero-price
  candle rejecting valid peers. Order maps now preserve operand decimal precision
  without lot-step snapping; default amendments use the same subtraction for
  both late-fill delta and replacement remaining. Integral scaled values above
  2^52 are not rounded by adding0.5; scaled overflow retains the finite difference.
- REST candles isolate typed invalid price/volume rows with row.skipped warnings
  and preserve sorted valid peers. A valid empty symbol array succeeds; missing
  symbol/envelope data, malformed fields, blank/negative/fractional timestamps and
  a nonempty all-invalid response fail visibly. Live candle validation is unchanged.
- Keep the one-native-order caller-ID contract, including protective/multi-leg
  rejection. An explicit leverage command continues verifying by write rather
  than assuming a possibly stale cache. Neither write behavior is broadened.
  Static private transport is already host-owned via Private/proxyConditional;
  no claim of live proxy-session validation is made.
- Validation: both original regressions fail before repair;390 focused ApeX/
  Binance tests pass, then the final3 unit tests pass including >2^52 integral,
  scaled overflow and subnormal operands (391 unique tests). Touched formatting,
  strict all-target Clippy and bans pass. Independent read-only review cleared
  runtime behavior; its optional extreme-arithmetic coverage was added.
