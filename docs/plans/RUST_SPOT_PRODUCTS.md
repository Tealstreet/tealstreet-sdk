# Hyperliquid and Kraken spot products

Branch: `feat/rust-spot-products`, based on `origin/feat/rust-rewrite`.
This extends the existing venue crates; TypeScript venues and registry defaults
remain in place. Explicit Rust selection must preserve saved spot accounts.

## Plan and verification

1. Public data: distinct spot constructors, native spot identities, typed
   streamed markets/tickers, full books, candles and socket recovery. Register
   WASM aliases and SDK selection; capture public tapes for each spot product.
2. Private data: native spot balances, currency/coin details, flat position
   settings, open orders, executions, polling/reconnect and stale-read fences.
3. Trading: whole-batch preflight, supported order shapes, client-ID contracts,
   native amend or safe replacement, cancellation and fixed spot settings.
4. Queries and consumers: match implemented versus inherited TS capabilities;
   exercise both aliases through actual compiled hosts and worker consumers.
5. Harness gates: documented private tapes, live public fixtures, both touched
   perpetual products and Binance regression gates, repository checks and CI.
6. Landing: merge current landing branch, PR into `feat/rust-rewrite`, independent
   code-correctness reviews posted to the PR, fixes/reviews until passing, merge
   only on green CI, then cut a fresh follow-up branch.

Every implementation phase receives an independent sub-agent review and a
stable commit with progress below. No stash, TS deletion, master changes,
rebases or force pushes. Gates retain their existing correctness/CPU/memory
thresholds. Credentialed private coverage uses documented scripted inputs and
must not be described as live-key verification.

## Product and authentication decisions

Hyperliquid spot shares the L1 action signer, sequenced writes and common
public feed transport with perps. It joins spot universe entries to token
indices, uses `10000 + universe.index` for native action assets, and spot
price precision. Wallet/application/vault identity and network choice must
remain consistent across REST, subscriptions and signing. Perpetual account,
builder, fee and HIP-3 discovery requests must not leak into the spot product.
Native spot balances need raw currency details plus USD valuation; failed or
older REST snapshots cannot overwrite newer socket state. Spot-only capability
decisions follow the actual TS spot methods and documented upstream support.

Kraken spot is a distinct state machine inside the existing Kraken crate:
`KrakenSpot::new(Options)`. Futures signing and WS challenge auth cannot be
reused as a spot protocol. Spot signs the exact form body including monotonic
nonce with `HMAC-SHA512(path || SHA256(nonce || body))`, decoded API secret,
and `API-Key` / `API-Sign`. Every private WS connection obtains its own REST
token before subscriptions; old tokens/responses cannot authenticate a new
generation. AssetPairs REST keys, altnames and WS symbols require explicit
identity indexing. Native `txid` stays canonical across atomic amend; generated
`cl_ord_id` does not imply support for caller IDs. Trailing placement uses the
documented positive relative distance; ambiguous edits fail before writes.

## Progress

- Planning: current brief/spec and TS spot contracts inspected; independent
  implementation lanes assigned to the two existing crates, with a separate
  harness lane. Host/consumer integration and gate orchestration stay with the
  parent. Constructor and auth plans audited before implementation.
- Gates, review findings, measurements and PR: pending.
