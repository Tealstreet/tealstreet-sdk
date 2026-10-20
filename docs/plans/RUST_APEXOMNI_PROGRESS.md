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
