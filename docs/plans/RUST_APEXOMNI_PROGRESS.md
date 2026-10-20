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
