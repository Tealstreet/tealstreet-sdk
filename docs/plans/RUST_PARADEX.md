# Paradex perpetuals Rust port

Branch: `feat/rust-paradex`, based on `origin/feat/rust-rewrite`.
Spec: `RUST_REWRITE.md`; process and mandatory gates: `VENUE_PORT_BRIEF.md`.
Full automatic execution; no stash, rebase, force push, or master changes.

## Phases and review gates

1. Public data: typed streamed markets and ticker seeds; BBO/trade/summary
   sockets; on-demand snapshot books, reconnect/watchdog and subscription
   recovery; REST candle reads and trade/poll candle subscriptions.
2. Private data/auth: StarkNet signatures, configuration, single-flight JWT
   acquisition/refresh, private socket auth/subscriptions, account reconciliation,
   settings and stale-snapshot protection. Review signing/auth separately.
3. Trading: sequenced jobs with complete preflight, exact caller identities,
   native amendments, conditional/bracket placement, conservative cancel chunks,
   leverage and explicit cross/one-way mode constraints. No uncertain-write replay.
4. Queries: cursor-bounded fills/order history/income/public trades and client-ID
   lookup in the engine's established JSON shapes; no store mutation from reads.
5. Harness gates/integration: wasm and SDK registration, independent signing
   verification, documented private tape, recorded/trimmed public fixture, CI
   matrix and size budget. Re-run Binance public and private gates.

Every phase: audit plan, implement, run focused verification, independent
sub-agent bug review, fix, verify and commit. Keep progress here. After all
mandatory gates pass: merge the latest landing branch, push, PR into
`feat/rust-rewrite`, post adversarial reviews, fix/repeat, wait for green CI,
merge and create a fresh follow-up branch.

## Signing/JWT design and audit checklist

- Real prod/testnet URLs only inside the venue. Reuse host transport routing:
  `createEngineTransport`/serialized native routing plan. Explicit public/private
  socket roles even though the upstream URL is shared. Paradex metadata enables saved-account tunnel defaults; the current routing
  table declares no Paradex WS override and resolves its real socket hosts.
  Preserve that policy; do not invent a socket proxy in Rust.
- Private mode uses key + main Paradex address + subkey public address, never
  secret. Public-only mode must perform no signing/account I/O.
- Fetch system configuration for the environment. Validate chain ID, credentials
  and felt bounds before auth. Reuse audited `safe-cex-crypto::starknet` primitives;
  typed domain/message structs or direct typed hashes, never a dynamic JSON tree.
- Match revision-0 SNIP-12 domain ordering: name, chainId, version. Authentication
  Request fields: method, path, body, timestamp, expiration. Sign POST `/v1/auth`
  with empty body, epoch seconds, expiration +86400 seconds; send to
  `/v1/auth/{subkeyAddress}` with account/signature/timestamp/expiration headers.
  Signature is a JSON string containing decimal r/s strings.
- Orders use epoch milliseconds; integer size/price quantum = truncate decimal
  string *1e8, without float multiplication. Order fields: timestamp, market,
  side(1/2), orderType, size, price. Native ModifyOrder appends the stable id;
  amendments cannot reuse placement signatures. Market signatures use price 0.
- Preserve the TS subkey path and audit Ethereum-derived account support against
  the SDK (key-length ambiguity must be documented rather than silently guessed).
  Remote signing must use the engine's host signing contract if supported; an
  unsupported mode must fail before any account/write I/O, never interpret a
  public key as a private key.
- JWT cache refreshes on demand after 180s (300s validity minus 120s buffer).
  Queue typed continuations behind one in-flight auth. Failed/malformed refresh
  fails pending operations without losing last-good account state. Bound queues,
  recover sockets with fresh auth, and never resend an ambiguous write. Validate
  the correlated WS auth response before subscribing; timeout and reconnect on
  missing auth/subscription acknowledgments. No credential/signature/JWT logs.
- REST failures and HTTP200 business errors never become empty account snapshots.
  REST starts capture revision counters; private events and accepted writes fence
  old snapshots. Emit one fill per execution, not cumulative order fill amounts.
- Independent oracles: current Paradex official docs, upstream paradex-py,
  vendored official JS SDK, existing StarkNet JS fixtures; deterministic synthetic
  keys only. Private tape validates real emitted signatures and outbound requests.

## Progress

- Initial checkout clean; fetched landing and created fresh requested branch.
- Read port brief/spec and Paradex runtime guidance; initial auth and harness
  plan audits running in independent sub-agents.
- SDK/wasm registration and credential/routing integration implemented. Focused
  integration verification: 199 TS tests, package typecheck/lint, wasm single-feature
  check PASS. These are provisional until independent phase audit completes.
- Public phase independent audit found malformed subscription acknowledgments,
  stale book snapshots and trade-versus-poll candle races; fixes in progress.
- Official Paradex asyncapi confirms snapshot feed depth15 only; full depth uses
  `order_book.{symbol}.deltas` with initial snapshot then deltas. Rust and TS
  reference will both use the full-depth protocol; declare the TS depth correction.
- StarkNet generic TypedData parser rejects the venue's legacy domain field
  ordering. Fixed-structure SNIP-12 hashing is being checked against independent
  StarkNet JS and upstream SDK vectors. No old dead request-signing facade ported.
- Trading and query continuations drafted separately, not wired into the public
  phase commit yet. No mandatory replay/performance gates claimed yet.

### First implementation checkpoint

- Corrected the TS reference full-depth book reducer and Paradex factory routing.
  22 depth/followup regression tests, 199 wrapper/routing tests, package typecheck
  and scoped lint pass. Public Rust review found/fixed ACK and stale-state bugs;
  final review is pending the binary-wire extension.
- Live probes now prove every prod/testnet JSON handshake fails with code40301,
  `SBE binary encoding required`. Explicit SBE versions1/2/3 negotiate; use v2,
  which has a published official XML schema. This is a real venue protocol change.
  Extend the engine with optional borrowed binary ingress and native/JS host
  forwarding; decode SBE in the typed venue state machine. The TS oracle gets an
  independent schema decoder. Record raw binary traffic, not fabricated JSON.
- Private/auth, trading and query drafts compile together; four independent
  signing corpus tests and the remaining Stark signing primitive test pass.
- Removed unused first-port generic Stark typed-data/felt parsing helpers and
  their obsolete fixture; crypto now depends only on starknet-crypto for these
  primitives. Venue-specific fixed hashing preserves Paradex domain field order.

### Reviewed data/auth checkpoint

- Phase1 public and phase2 private/signing independent reviews PASS after fixes.
  Native verification: 23 public +16 account +4 signing tests, plus6 unit tests
  (SBE/price-band/tradingcorrelation). Phase3 trading currently12 tests pass;
  phase4 queries7 tests pass; their independent reviews are still underway.
- SBE schema1:2 is decoded directly into typed Rust state; JS/native hosts expose
  explicit binary mode and borrowed ingress. The JS host suite passed36 tests;
  the review also caught an older-engine physicalsocket leak and native fallback
  health omission, nowfixed with additional regressions awaiting final checks.
- Independent TS codec uses official Python-generated vectors across8 templates;
  public/private null, enum, root/group/string bounds and rapidcrosssymbol updates
  reviewed. Paired TS corrections retain full depth, every market's BBO/summary,
  candle generation fences and final CLOSED statuses.
- Privateauth preserves environmentconfiguration, exact SNIP12 domains and
  deterministic decimalquantums. JWT queue singleflights and fails pending typed
  operations explicitly; missingauth/subscription acks recover; serverstamps and
  terminalposition/order fences block old REST/event resurrection. One fill emits
  per executionID, retaining distinct economicallyidentical partials.
- Shared maxSlippagePct input now survives deserialization. Paradex cappedmarket
  uses a freshdepth5 read then constrained signed LIMIT/IOC; other venue builders
  receive only a mechanical None default. No other venue behavior changed.
- Proxy JWT formerly overwrote Paradex's venue Authorization when proxied. The
  shared TS policy now uses Tealstreet-Authorization for Paradex, preserving both
  credentials identically for the native resolvedplan and JS host.
- No replay, performance, finalCI or PR/merge gate has been claimed yet. Harness
  registration and live20second publicrecording are underway.
