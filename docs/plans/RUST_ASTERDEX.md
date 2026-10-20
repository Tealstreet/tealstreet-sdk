# AsterDEX Rust port

Branch: `feat/rust-asterdex`, based on `origin/feat/rust-rewrite`.
Spec: `RUST_REWRITE.md`; process and gates: `VENUE_PORT_BRIEF.md`.
Full automatic execution; stable commits, no stash, PR targets the rewrite branch.

## Phases

1. Public data: crate, typed streamed market/ticker bootstrap, open-interest
   polling, aggregate ticker/mark streams, validated partial-book snapshots,
   candle reads/subscriptions, environment URLs and socket watchdog. Share
   identical Binance wire records, not Binance depth sequencing or policy.
2. Private data: auth, clock, listen-key lifecycle, account/position/mode/orders,
   per-market settings, split coalesced repair and stale-snapshot guards.
3. Trading: sequential regular/split/bracket orders, conditional close-position
   semantics, cancel/replace, idempotent unknown cancel, settings, builder fees
   and V3 STP. Reject caller client IDs and unsupported trailing inputs before I/O.
4. Queries: fills with the existing realized-PnL delta contract, order history,
   income and public trades in engine query shapes; typed response decoding.
5. Harness and gates: WASM/adapter/recorder registrations, independent auth
   oracle and documented private tapes (V1 and V3), recorded public fixture,
   strict correctness/performance and Binance regression, CI roster.

Each phase: audit plan, implement, verify, independent sub-agent code-correctness
review, fix, commit progress. Final: merge latest landing structure, run all
gates, open PR, post adversarial code-correctness reviews, address/repeat, wait
for green CI, merge, and cut a fresh follow-up branch.

## Authentication design

- Resolve explicit `extra.asterdexApiVersion` first; otherwise select V3 only
  for a valid user address, signer address and 32-byte private key. Preserve
  auth-field precedence over flat applicationId/key/secret. Wallet-shaped
  credentials missing the main wallet fail with the existing actionable hint.
- V1: `X-MBX-APIKEY`, business params then recvWindow=20000 and corrected
  timestamp, HMAC-SHA256. All signed verbs use query parameters, no body.
- Standard V3: canonicalize business params plus recvWindow/timestamp into
  sorted string-valued JSON. ABI encode `(string,address,address,uint256)`,
  hash with Keccak, EIP-191 hash those 32 bytes, recoverable secp256k1 signature.
  Append checksummed user/signer and a monotonic microsecond nonce to the query.
- Aster Code V3 (`POST order`, `GET builder/agent`): exact insertion-ordered,
  unencoded query, business params then asterChain/user/signer/nonce. EIP-712
  `Message(string msg)` under AsterSignTransaction v1, zero verifying contract,
  chain1666 mainnet/714 testnet. Signed bytes equal sent bytes before signature.
  Keep the TS split explicit even where newer docs describe EIP-712 generally.
- Reuse `safe-cex-crypto` Ethereum hash/sign/address and EIP-712 primitives.
  Keep venue ABI/preimage construction in the venue; no JSON value trees.
- Nonces increase even on equal/backward clock samples and across retries.
  Clock sync prefers selected version, falls back to the other, retains good
  offsets on failure, refreshes every five minutes. Only proven signature or
  timestamp rejection can resend a write; ambiguous writes never retry.
- Secondary read-only signer shares the owner wallet and selects separate
  read credentials/host egress; writes retain the primary signer. Inspect and
  extend current host secondary-key rejection narrowly, preserving defaults.
- Builder attribution is V3 only. Current mainnet default is the concrete
  builder address in TS, maker/taker rate0.0001; testnet placeholder disables
  it. Validate exact decimal strings (max0.001), zero-rate omission, per-order
  maker/taker selection and propagation to every child. Owner-wallet builder
  approval/agent registration remain onboarding operations, never synthesized.
- Independent tests must verify signatures/preimages using official documented
  examples and ethers, including decimal strings, testnet domains, field order,
  nonce monotonicity, missing/invalid keys, clock/rejection recovery and read/write
  separation. No real credentials enter fixtures or commits.

## Progress

- Planning: read latest brief/spec, parent and Aster guidance, inspected TS and
  Binance engine. Two independent read-only audits cover auth and venue surfaces.
- Public phase complete: typed streamed bootstrap, partial snapshots, OI fallback,
  subscriptions/watchdog and candle reads. Native public tests10/10; signing unit
  tests5/5 (eight independent ethers/HTTP-serializer vectors); crate clippy and
  fmt pass. Independent code-correctness review found only initial OI timing;
  fixed first background attempt to250ms with bounded symbol fallback.
- Signer exact-byte oracle exposed query square-bracket serialization differences;
  matched the actual HTTP serializer rather than normalizing requests.
- Private/trading/queries implementation underway; their reviews and full gates
  are not yet claimed.

### Shared frame and command preparation

- Added optional typed currency balances after the existing collateral tail;
  scalar and coin-only frames retain their byte layout. Engine tests50/50 and
  decoder compatibility/value/clear tests pass. Combined host suite needs the
  combined WASM rebuild (the prior Bitget-only artifact cannot run Binance).
- Added typed STP policy/commands and optional placement field. Existing venue
  literal construction defaults to no policy; workspace all-target check passes.
- Private/trading phase code-correctness reviews caught and fixed flip-side,
  account overlap, fill identity, terminal cancel races, and hedge closure bugs.
  Trading review is clear; final integration gates remain pending.

### Private, trading and queries checkpoint

- Private implementation and independent review complete: V1/V3 auth, listen
  keys/watchdogs, strict account event time, per-domain repairs, terminal races,
  currencies and settings. Native private tests16/16 pass (including account
  identity and configured leverage commands).
- Trading phase independently reviewed and fixed: sequential split/bracket
  orders, builder/STP policy, cancel/replace fill races, idempotent unknown cancel,
  settings ACK versus stale REST. Native trading22/22 plus decimal unit pass.
- Query phase independently reviewed:16 native tests pass. Documented string
  ledger IDs, PnL normalization before invalid-size filtering and overflow,
  V3 account-wide history, complete/deduplicated/paced income range walks.
- Public native tests12/12 and strict Aster all-target clippy pass. REST depth
  read preserves the existing standalone quote-total contract; stream totals
  retain their base-quantity contract. Public ticker identity tail preserves
  orphan rows with no active market; Store11/11, decoder7/7 and hydration/delta
  regression pass. No synthetic markets or result filtering.
- Secondary routing:192 TS transport tests, native selector test, package
  typecheck and source lint pass. Root typecheck44/44 and lint28/28 pass.
- Initial artifact977616bytes (budget1258291). V1 and V3 private gates pass on
  documented tapes; native V3 contracts pass before latest metadata commands.
  Live public recording20seconds:383WSframes/twobooks,608markets/631tickers.
  First public gate failed only23 orphan ticker identities; performance passed
  (TS/Rust main134.1/86.2ms, steady60.9/38.6ms, retained8.1/1.9MB). Correctness
  fix is implemented; final artifact rebuild/gates remain pending.
- Intentional TS corrections: hedge-zero closes only the reported rail;
  historical candles use shared seconds; amendments stop after a fill during
  cancel; nonce state is per agent (official V3 contract), not per owner+agent.
- Remaining phase5: public read retry and exact direct/CORS clock fallback
  review findings, final rebuilt replay/host/Binance gates, CI and PR reviews.

### Harness and transport review checkpoint

- Registered Aster in WASM, wrapper, recorder, replay, private generators and CI.
  Independent V1/V3 validators bind exact native signatures, declared signer,
  nonce uniqueness, V1 recvWindow and strict future skew. Harness161 tests and
  wrapper11 tests pass; no economic/result normalization was added.
- Public-read retries now match TS: bounded read-only1/2/3second retries,
  including native429/503 bodies, with streamed accumulator resets. The clock
  tries preferred/alternate direct then preferred/alternate public CORS, each
  with a5000ms deadline. Native URLs stay in the venue; typed route/deadline
  effects are consumed by hosts and absent from wire/tunnel requests.
- Independent request/host reviews found and closed negative-code retry ordering
  and20-second clock deadline regressions. All84 native Aster tests pass;
  final clock/retry18/18 and strict clippy pass. Host route/disposal tests pass.
- Root test attempt failed only timeout cases under shared Mac load~175:
  HyroTrader configuration imports, capability branding imports, and registry
  construction.464files/6853tests passed in the reported safe-cex task. Preserve
  this failed evidence; it is not a passing gate. No timeout/assertion was weakened.
- Next: commit checkpoint, merge latest landing (WOO/Lighter), run final artifact
  gates and complete CI/PR code-correctness review loop.

### Landing integration

- Merged landing31ec401f60 (WOO, Orderly and Lighter) after checkpoint076496fbb1.
  Combined additive registrations/tests/CI and retained incoming async host
  cancellation, single-use wrapper startup, position IDs and signed-packet guards.
- Three independent integration reviews resolved assigned conflict scopes. All
  sixteen venue features and both hosts compile; engine53 tests, incoming WOO
  and Lighter suites, wire8 tests, WASM native2 tests, host25 tests, harness212
  tests and wrapper35 tests pass. Package typecheck, scoped lint and syntax pass.
- Final integrated artifact build/replays and broad CI are next; previous
  single-lane numbers remain explicitly preliminary.
