# TxFlow Rust port

Branch: `feat/rust-txflow`, based on `origin/feat/rust-rewrite` at
`7da56dc8a552c595c6844c512ef1de54ff24225b`. Follow `VENUE_PORT_BRIEF.md`.
Mainnet, preview and production availability remain disabled. Do not stash,
rebase, force-push, or target master.

Status: production registries select Rust; all local gates, phase reviews and
fresh PR code-correctness re-reviews pass. [PR #1329](https://github.com/Tealstreet/tealstreet-sdk/pull/1329)
targets `feat/rust-rewrite`. Require green exact-head CI, merge, then create a
fresh follow-up branch. Latest post-review measurements are at the end.

## Initial audit and verification constraint

Read the TxFlow `CLAUDE.md` before the implementation. At the initial audit,
`txflow.exchange.ts` was a Phase 0 scaffold whose `start()` unconditionally
rejects with `TXFLOW_PHASE_0_UNIMPLEMENTED_MESSAGE`. It had no public, private,
trading or query implementation. This is a missing reference implementation,
not evidence that the venue lacks these APIs.

On 2026-10-20, fetched the official [complete API documentation](https://docs.txflow-testnet.xyz/llms-full.txt)
and successfully read testnet `POST /info {"type":"perpMeta"}` with
`x-api-version: 2026-08-16`. The response was HTTP 200 and
`success: true, code: 0`; BTC and ETH have explicit indices 1 and 2. No writes,
funding, agent approvals, or mainnet calls were made.

The brief requires a live public recording, private documented-payload tape,
identical TS/Rust replay output, and lower Rust CPU and memory. A functional
Rust adapter cannot pass those comparisons against an adapter that refuses
startup. Do not alias the Hyperliquid TS adapter as TxFlow, normalize away the
missing reference state, benchmark startup rejection as public traffic, add a
gate skip, or report these gates passing. Functional landing requires a real
testnet reference adapter or an explicit change to the verification contract.

An independent sub-agent reviewed this audit and the signing/reuse plan. It
confirmed the oracle constraint and the protocol differences below. The port
includes the missing testnet TS reference as a prerequisite to its replay
gates. It remains testnet-only with all production/preview flags disabled.

## Reuse audit

Hyperliquid is the closest existing Rust venue, but TxFlow is not a safe
configuration-only variant. Reuse the MessagePack named-map action hash
preimage implementation from `venues/hyperliquid/src/sign.rs`, extracting a
shared primitive only when both production callers exist. Reuse
`safe-cex-crypto::{ethereum,eip712}`, engine `Book`, streamed array decoding,
typed Store, socket identities/watchdogs, and paging infrastructure.

Do not copy Hyperliquid account/trading/history state machines: the domains,
signing schema, action fields, symbol identities, websocket topology, client
IDs, acknowledgement semantics and pagination differ. Preserve all existing
Hyperliquid vector and replay gates after any shared extraction. Audit crypto
call sites before deleting anything; do not introduce unused signing helpers
or retain first-port leftovers without callers.

## Signing and authentication design

Sources: [Signing](https://docs.txflow-testnet.xyz/api/signing.md),
[WebSocket API](https://docs.txflow-testnet.xyz/api/websocket.md),
[Exchange API](https://docs.txflow-testnet.xyz/api/exchange.md),
[Rate limits](https://docs.txflow-testnet.xyz/api/rate-limits.md).

- Account identity is the main wallet in `applicationId`; `secret` is a locally
  held, already-approved agent private key. Never use the agent address as
  `user`. Reads and subscriptions are unsigned. Validate the wallet/key before
  private I/O and keep credentials out of logs and committed fixtures.
- Domain: `TxFlow-Testnet`, version `1`, chain ID `2511`, zero verifying
  contract. The documented domain separator is
  `839dd579adc05b1545a37213db5dd3dc6928532ddf73dfa1b53f647421060a8f`.
  No executable mainnet signing path while mainnet is offline.
- Trading Agent type is
  `Agent(string txflowNetwork,uint32 chainId,uint32 apiVersion,bytes32 connectionId)`.
  The corresponding struct carries network `TxFlow-Testnet`, chain `2511`,
  signing API version `1`. HTTP version `2026-08-16` is a separate concept.
  Hyperliquid's `Exchange`/`1337` domain and `source` Agent schema cannot be used.
- For ordinary agent actions, encode a typed MessagePack named map WITHOUT
  action `type`. Preserve the exact documented declaration order, never sort
  JSON keys or round-trip through an untyped tree. For `order`, outer fields
  begin `grouping`, then `orders`; inner order fields are
  `a,b,p,s,r,t,c,m,expiresAt`. Omit absent optional fields. Prices/sizes retain
  their exact string spelling, asset IDs remain integers, and address-valued
  MessagePack fields use binary bytes.
- Append nonce as eight big-endian bytes, then a zero vault flag or one plus
  twenty decoded address bytes; Keccak-256 gives `connectionId`.
  `vaultTransfer` always uses the zero flag. `claimBuilderRewards` retains its
  action type in its preimage. Neither is a substitute for an ordinary action.
- EIP-712 uses the TxFlow domain and Agent struct above. Reuse crypto digest
  signing and canonical low-S signatures, returning 32-byte `r`, `s`, and
  JSON `v` 27 or 28. `signatureChainId` does not override agent signing.
- Nonces are milliseconds and unique per signer. Allocate
  `max(floor(now_ms), previous + 1)` with checked arithmetic and validity
  bounds; reject non-finite time or exhaustion before writes. Lost responses
  never trigger blind resubmission. HTTP 429 specifically means no execution;
  respect `Retry-After` and use a fresh nonce if a bounded retry is implemented.
- Every `/info` and `/exchange` request has `x-api-version: 2026-08-16` and JSON
  content type. Public reads carry host routing hints; signing is local and
  real venue URLs stay in venue code.
- Direct account-key actions (`approveAgent`, fund transfers, withdrawals,
  builder approval/revocation) use distinct primary types. An agent key cannot
  sign them. The connector must not approve or fund accounts automatically.
  Builder fees require independently confirmed account approval; do not inherit
  Hyperliquid's builder identity or assume its fee approval is portable.
- Verify before enabling I/O: official order vector (nonce `1700000000000`,
  no vault) has connection ID
  `6546d1c3acf430a28a0b2552c66e985f6543646660874734732ae9e9daf4d806`,
  digest `eae1ae63709636fb9567accd33bc5dd2717bfac4d97e9a853f91c7fb391e1cc6`.
  Check the published `r/s/v`, the direct `approveAgent` vector, named map bytes,
  optional omission, field order, integer asset representation, exact decimal
  strings, vault separation and monotonic nonce behavior. Use the official
  explicitly disposable vector key only; never test against real credentials.

## Phases and review checkpoints

### 1. Public data

Implement `sdk-rs/venues/txflow` with typed wire structures. Reject non-testnet
startup before any I/O. Stream `perpMeta` from `data.universe`; map explicit
`index`, accept USDC crypto perps, drop delisted markets, mark halted markets
non-tradable, preserve `useNotionalMargin` metadata, and use `priceTick` and
`szDecimals`. Resolve request `coin`/`asset` as numeric strings and native order
asset fields as integers, independent of display names.

Seed and stream tickers from documented native fields; funding percent must
be converted to engine fractional units. Books retain full native depth and
respect the documented snapshot/delta contracts, event time and reconnect
fences. Do not borrow Hyperliquid sequence assumptions. Candles and history
use the documented interval and timestamp conventions.

Use one multiplexed node `/ws` feed and an independently identified business
`/api-ws` feed, tagged with real roles and engine feed identities. Node
subscriptions repeat channel type at the top level and inside subscription;
API-service subscriptions omit the top-level type. Both require fixed
10-second pings, even under continuous incoming traffic; recognize node
`{"method":"PONG"}` and API-service `{"channel":"pong"}`. Reconnect invalidates
books, restores active subscriptions and uses engine timers/watchdogs.

Verify documented payload mapping, stale frames, filtering, header/routing,
watchdog/reconnect behavior, full books and candles. Sub-agent review, fix,
targeted tests, fmt/clippy and phase commit.

### 2. Private data

Implement the signing design above, unsigned account-address reads and
private socket subscriptions. Stream typed clearinghouse state, native open
orders and per-market settings. Commit authoritative empty state only after
successful valid reads. Fence reads against intervening stream events, accepted
writes, disconnect generations and disposal. Keep settings separate from open
positions; never create zero-size positions for configuration.

The API-service `userFills` subscription is a historical snapshot, not a live
execution stream. Reconcile native REST executions by execution ID with a
startup baseline and reconnect deduplication; never fabricate execution prices
from cumulative order updates. Exactly one engine fill per native execution.
Account refresh cadence must be defined from documented native behavior, since
the scaffold has no TS tick to preserve.

Verify official signing vectors and deterministic account race/reconnect/error
tests. Sub-agent review, fixes, verification and phase commit.

### 3. Trading

Preflight complete batches before any write. Define native supported order
shapes from verified docs and retain fail-closed controls for unmeasured
reduce-only auto-cap, partial TP/SL and infinity-stop semantics. TxFlow cloids
are at most 64 bytes and cannot begin `0x`; Hyperliquid's cloid generator is
incompatible. Honor caller IDs verbatim only on exactly one native order;
otherwise reject before writes. Mint distinct valid native IDs for generated
orders and bracket children.

Use serialized jobs and typed actions for order/cancel/cancelByCloid/modify2,
leverage/margin and documented position mode. Batch limits are 50 orders and
200 cancels. A single `statuses:["success"]` applies to the WHOLE action, not
one row per order. Consensus acceptance does not establish execution or order
IDs. Request `confirm:"executed"`, distinguish HTTP 202/pending, and resolve
identity through authoritative order updates/reads keyed by cloid. Never
manufacture success IDs or retry uncertain writes. `modify2` preserves native
identity and cumulative filled; omitting `tpsl` clears existing children, so
preserving a bracket requires explicitly preserving its pair.

Verify all emitted signatures independently in the tape transport, including
caller IDs, optional key omission, native fields, brackets, pending outcomes,
semantic rejects, lost replies and read/write races. Sub-agent review, fixes,
verification and phase commit.

### 4. Queries

Enumerate documented `/info` fills, historical orders, funding, ledger and public
trade surfaces. Use offset/`hasMore` continuation and typed completeness checks;
convert milliseconds to the existing rare-result JSON shapes at the boundary.
Preserve native IDs, execution units, fee signs and currencies. Indexing lag
cannot prove an order was never placed. Expose unsupported reported surfaces
explicitly; never label a derived fill fold as venue-reported closed PnL.

Verify payload and pagination behavior, malformed/failed pages, duplicate IDs,
coverage and range boundaries. Sub-agent review, fixes, verification and commit.

### 5. Integration and gates

Only register functional Rust capability after its reference and core behavior
exist. Add the Wasm feature/constructor, worker venue mapping, harness venue,
private tape generator/root script, per-venue build and CI matrix/budget. Keep
availability flags unchanged. Retain the TS oracle through the gate run; apply
the spec's deletion rule only when the port actually ships and the harness has
an explicit supported replacement reference strategy.

Run all gates in the brief: bans, fmt, native clippy/tests, wasm32 clippy/build,
signed private replay, a live 20-second two-book public capture and trimmed
fixture, exact output and lower CPU/memory, Binance public/private regression,
root typecheck/lint/tests and host integration checks. No placeholder performance
numbers. Review the phase with a sub-agent, fix and commit evidence.

Then push, open PR into `feat/rust-rewrite`, run fresh independent adversarial
code-correctness reviews and post findings on the PR, fix/repeat, merge current
landing structure, rerun affected gates, and merge only on green exact-head CI
and all local gates passing. Cut a fresh branch from fetched landing afterward.

## Progress

- Initial source/API/reuse audit complete; independent review confirms plan.
- Implementing the missing testnet TS reference alongside Rust so the required
  comparisons exercise real native behavior. Public mapping, private
  reconciliation and trading have deterministic tests; phase reviews ongoing.
- Signing phase independently reviewed: exact official domain, MessagePack
  connection ID, digest and signatures match; four native tests pass. Reuses
  Hyperliquid preimage hashing directly; no copied parent venue state machine.
- Functional replay/performance gates pending; no passing replay gate or
  production capability is claimed.

### Reviewed implementation checkpoint

- Public phase: independent review accepted after fixing duplicate metadata,
  pre-metadata subscription intent loss, wrong-instrument REST seeds and atomic
  seed readiness. Native public tests 15/15, TS public tests 5/5.
- Private phase: independent review accepted after preserving executions in the
  first response after startup, learning flat-account hedge mode from native
  settings, and bounding retired order identity state. Native account 12/12,
  TS account 7/7.
- Trading phase: independent review accepted after fixing discarded pure order
  fetches and queued-amend protection clearing; native trading 9 integration
  tests plus 5 unit tests pass. Unknown/pending execution acknowledgements never
  authorize a retry. Scalar hedge leverage is explicitly refused.
- Query phase: review findings fixed (local bounds/symbol filters, empty ledger
  identities, unknown status rejection and funding-leg identity collisions).
  Native history 6/6, independently authored TS history proofs 14/14. Ledger
  income additionally requires documented completed settlement status.
- Signing: 4 native official-vector proofs and 2 TS proofs. Full safe-cex
  typecheck passes. Client-ID regression exposed synchronous guard throws; the
  TS overrides now reject asynchronously while emitting before any write.
- First live public recording: 20 seconds, 78 markets/tickers, two full books.
  Public replay PASS: CPU TS 165.1 ms / Rust 59.7 ms (2.77x), retained memory
  TS 14.6 MB / Rust 1.2 MB (12.05x), single-venue Wasm 896,428 bytes.
- Private tape: 19 WS frames, 15 commands, one close, 8 signed writes, exactly
  one expected execution. Initial gate matched all account/writes/query results;
  one preflight error-text mismatch is being corrected without normalization.
- Harness generic startup-clock option keeps deterministic private session
  timestamps and existing default behavior. Native recorder explicitly declares
  direct egress only for public-CORS reads; browser/account routing remains
  configured and fail-closed. Independent harness protocol review in progress.
- Shared crypto audit: no helper becomes unused through this port; it directly
  reuses the existing Hyperliquid preimage and Ethereum/EIP-712 primitives.
  Removed the redundant direct TxFlow MessagePack dependency.
- Remaining: final gate review, full root/host/workspace checks, latest landing
  merge, PR code-correctness reviews, green exact-head CI, merge and follow-up
  branch. No merge or completion claim yet.

### Landing integration checkpoint

Merged landing commit `1f9e9a5206` (Paradex and ApeX Omni). Retained incoming
workspace layout, binary host/engine support, routing, features, constructors,
client-ID rules, harness profiles, CI rows and roster entries. An independent
code-correctness review caught TxFlow missing from the incoming conditional
Wasm `secret` field; corrected and verified the single-venue constructor build.
The landing removed unused dynamic EIP-712 production helpers: TxFlow's
independent test oracle now imports ABI decoding as a dev dependency rather
than restoring those helpers. Production signing still uses only prehashed
EIP-712 and the reused Hyperliquid preimage.

Merged-state checks: all-feature native Wasm check, TxFlow-only check, all
TxFlow tests, full safe-cex typecheck, and 323/323 harness tests pass. Independent
integration review accepted. Release rebuild and final replay/whole-workspace
checks follow this commit; the interrupted pre-merge build is not gate evidence.

Root typecheck (44 tasks), lint (28 tasks), bans, workspace formatting and
workspace Clippy pass. Root tests exposed the old scaffold's mainnet/default
transport snapshot; updated only the TxFlow entry to its testnet endpoints and
installed account/public transport adapters. The focused characterization now
passes. The full root suite and Rust workspace tests are running again.
`KNOWN_TS_FIXES.txflow` documents paired fixes without normalizing replay output.
The committed feature range passes the credential scan.

Pre-Phemex release checkpoint: TxFlow-only Wasm is 871,564 bytes. Public gate
passes (125 frames, 78 markets/tickers, two books; three runs): CPU TS 93.7 ms /
Rust 61.1 ms (1.53x), retained memory TS 14.0 MB / Rust 1.2 MB (11.35x).
Private gate passes all 35 frames, 15 commands, eight writes and one execution.
Its first rebuilt attempt exposed a harness representation error: TxFlow stores
buy/sell leverage, but its profile selected scalar comparison. Enabling the
existing independent-leverage comparison preserves and compares both rails;
neither implementation nor its output is rewritten. Merge Phemex next, retain
the reviewed union, then rerun combined TxFlow/Binance and host gates.

Merged Phemex landing `45d7a3fb98` after the pre-merge checks completed. The
independent integration review compared the resolved index against both parents
and accepted the complete union, including all Wasm option guards, shared host
byte APIs, bootstrap routing, deterministic private clock and caller-ID checks.
Locked isolated TxFlow/Phemex constructor checks and workspace formatting pass.
Pre-merge workspace tests passed 1,835 tests; root tests passed all 27 tasks.
Final combined release/replay and merged-state checks are in progress.

Final harness phase review found and fixed three proof gaps: compare retained
client IDs/time-in-force; require a cloid on every connector order before any
generated-ID normalization (the raw official no-cloid vector stays valid); use
zero-based private socket ordinals so startup/reconnect cannot depend on an
accidental extra watchdog reconnect. Independent regressions verify all three,
including the actual generator and a two-connection virtual replay. Review
accepted, 347 harness tests pass on Node 24; Node 22 verification is running.
A native limit/market batch proof verifies distinct minted cloids (13 trading
tests pass). These are harness/test changes, with no Rust production change
after the combined release build started.

### Complete gate checkpoint before Coinbase integration

All required gates pass on `59d6814c66` with Phemex merged: bans, formatting,
workspace Clippy/tests (1,929 tests), Wasm Clippy, 347 Node 22 harness tests,
77 Wasm wrapper tests, 60 host tests, root typecheck/lint/tests (44/28/27 tasks).
The generated-cloid regression additionally passes the 13-test trading suite
and venue Clippy. An over-parallelized JavaScript check hit wall-time deadlines;
the unchanged suites pass sequentially after compilation settles.

Combined-build TxFlow private replay: PASS, 35 frames, 15 commands, eight writes,
one execution, stored caller IDs/time-in-force and both leverage rails compared.
Combined public replay: PASS, 125 frames, 78 markets/tickers, two books,
TS/Rust CPU 69.8/51.3 ms (1.36x), retained memory 14.0/1.9 MB (7.24x).
Binance regressions: public PASS (873 frames, 917 markets, 783 tickers, three
books; CPU 194.9/95.5 ms, memory 24.7/3.2 MB); private PASS (14 frames).

The final fetch exposed Coinbase landing `5c0e437faf`; merge its current host,
transport and harness changes before PR creation and repeat affected checks.

Coinbase integration review accepted the resolved union: both authentication
paths, Wasm feature guards/constructors, routing, shared balance frame writer and
reader, profiles, CI rows and replay contracts remain intact.

### Production cutover after passing the replay gates

The spec requires deleting the production TypeScript implementation when its
Rust port ships. Move the new replay reference to
`src/__tests__/reference/txflow`, preserving its implementation and comparison
behavior; update its relative imports and audit-test/harness paths. Route both
production registries to `RustEngineExchange`. Extract only the small shared
client-ID validation/minting rule for the wrapper; no production dependency may
point into the test reference. Keep availability/measurement flags disabled.
Update the transport characterization to Rust's absence of Axios instances,
add a registry cutover proof, and run an independent cutover review followed by
root, Rust, host and replay checks on the merged result.

Cutover review accepted: all nine modules preserve behavior, all 42 relocated
imports resolve, production imports only the small shared client-ID rule, and
both registries select Rust. Full safe-cex typecheck, 34 focused proofs and the
updated TxFlow transport characterization pass. An actual Wasm startup proof
through the production registry is being added separately. The Coinbase-merged
release build and workspace checks continue; no Rust production edits followed
the merge.

Production registry real-Wasm proof passes 2/2: empty API key with distinct
wallet/agent credentials, markets/tickers/account loading, flat-account leverage
7 bridged independently of positions, wallet subscriptions on correctly tagged
native sockets, required headers/public routing, and mainnet rejection before
I/O. Full merged-state checks and final combined replay follow.

### Final local acceptance (Coinbase landing included)

- Bans, workspace formatting/Clippy/tests: PASS, 2,047 native tests.
- Combined and isolated TxFlow Wasm Clippy: PASS.
- Root typecheck/lint/tests: PASS, 44/28/27 tasks; safe-cex 7,264 tests.
- Package typecheck including the new Wasm proof: PASS.
- Harness 375/375, host 63/63, Wasm wrapper 85/85: PASS.
- Credential scan across the feature commits: PASS, no leaks.
- Combined TxFlow public: PASS, 125 frames, 78 markets/tickers, two books;
  TS/Rust CPU 98.7/78.8 ms (1.25x), memory 14.1/2.0 MB (7.10x).
- Combined TxFlow private: PASS, 35 frames, 15 commands, eight signed writes,
  one execution; caller IDs/time-in-force, both leverage rails and queries exact.
- Binance public/private regressions: PASS; public 873 frames, CPU 196.3/99.1
  ms (1.98x), memory 19.2/3.3 MB (5.89x); private 14 frames.
- Final isolated TxFlow bundle: 875,675 bytes (budget 1,258,291).
- Final isolated public (three runs): PASS, CPU 89.8/65.2 ms (1.38x), steady
  CPU 13.2/7.8 ms (1.69x), memory 14.1/1.3 MB (11.25x), linear Wasm 0.5 MB.
- Final isolated private: PASS, all 35 frames; no output normalization.

No live authenticated writes were performed. The original TS scaffold had no
functional behavior to compare; the new documented-payload reference is frozen
under tests. Paired stale-read, fill-deduplication and uncertain-ACK protections
are documented in `KNOWN_TS_FIXES.txflow` without rewriting either result.
Mainnet/preview remain disabled. Unmeasured reduce-only, conditional/bracket,
notional-margin and protective semantics remain rejected, and undocumented REST
public-trade history remains explicitly unavailable. These limits are documented
in the venue CLAUDE and are not enabled by this port.

### PR integration and final correctness review

[PR #1329](https://github.com/Tealstreet/tealstreet-sdk/pull/1329) targets
`feat/rust-rewrite`. Its initial head passed every local gate above. A new
Polymarket landing `a0c11afd87` conflicted with shared registration rows;
resolved the complete union, retaining prediction sections/types and both
venues' authentication, Cargo/features, CI and harness behavior. Independent
integration code-correctness review accepted the resolved index against both
parents. Locked metadata, workspace formatting and isolated TxFlow constructor
checks pass. Core TxFlow production code is unchanged. Fresh independent PR
reviews cover public/signing/auth, private/trading, and history/harness; rerun
merged-artifact gates, post findings/fixes, then require green exact-head CI.

Fresh PR code-correctness reviews found four P2 issues, posted as separate
public/auth, private/trading and history/harness comments: sparse contexts erased
volumes; position snapshots overwrote configured hedge rails; the wrapper
discarded unsupported order-history filters; delisted instrument identity was
missing from account-wide history. All are being corrected and re-reviewed.
The wrapper now rejects ID/status filters before command dispatch; three
regressions pass (9/9 wrapper tests). Historical filtering after a native limit
would be incorrect, so this unsupported option is explicit and documented.
The PR's initial TxFlow replay CI passes. An unrelated Phemex private balance
mismatch is under separate investigation; no merge until exact-head CI is green.

All original findings and one adjacent REST-mode fence finding are fixed and
accepted by their original independent reviewers. Paired TS/native corrections
preserve sparse volumes (including the actual recorded BTC row), keep position
effective leverage separate from authoritative market rails, advance the mode
fence after validated REST settings, and retain delisted history identities
outside tradable markets. Native account 19/19, public/history 26/26; TS account
14/14 and public/history 24/24 pass. Public/history Clippy, formatting and full
safe-cex typecheck pass. Wrapper guard proofs pass 9/9. Refresh the public tape
from a real recording because metadata now requests delisted identities; do not
rewrite the old recorded request. The generic TS replay snapshot boundary is
also being corrected before rerunning the Phemex CI regression.

Shared replay-boundary fix independently accepted: both implementations now
synchronously detach every comparison/protocol result before invoking memory
measurement. CPU includes reconstruction/copy work symmetrically and the copies
remain retained during memory sampling. Three deterministic regressions mutate
balance 1280→1300 and nested/typed-array state synchronously and asynchronously
during measurement; captured results remain unchanged. Complete Node 22 harness
passes 387/387. No fixture padding, Phemex normalization or gate relaxation.
The exact CI log cannot distinguish a pre-boundary REST overwrite from the
confirmed post-boundary exposure; rebuilt Phemex private replay is still required.

### Post-review acceptance

All reviewer findings and the adjacent REST settings fence are corrected and
accepted. Refreshed the actual 20-second live public recording: 202 retained
records (79 HTTP, 120 WS, two opens and metadata), `includeDelisted:true`, 78
tradable markets/tickers and two books. No recorded request was fabricated.

- Native TxFlow 67/67 and engine 67/67; bans, workspace formatting, strict venue
  Clippy and combined/isolated Wasm Clippy: PASS.
- Root typecheck/lint/tests: PASS, 44/28/27 tasks; safe-cex 7,308 tests twice.
- Harness 387/387, host 64/64, real-Wasm wrapper 101/101: PASS.
- Combined TxFlow public/private: PASS; public CPU TS/Rust 84.7/67.5 ms (1.26x),
  retained memory 14.3/2.1 MB (6.79x). Private still has 35 frames, 15 commands,
  eight signed writes and one execution.
- Binance public/private: PASS; public CPU 280.4/167.6 ms (1.67x), memory
  20.9/4.8 MB (4.34x), three books; private 14 frames.
- Phemex private regression: PASS twice at unchanged output, resolving the
  initial CI failure without venue normalization or fixture padding.
- Final TxFlow-only Wasm: 887,206 bytes (budget 1,258,291).
- Final isolated public, three runs: PASS, 122 replay frames; TS/Rust CPU
  67.1/46.8 ms (1.43x), steady 8.2/6.3 ms (1.29x), retained memory 14.3/1.3 MB
  (10.65x), linear Wasm 0.5 MB. Final isolated private: PASS.

Independent findings and passing re-reviews are posted on PR #1329. Remaining
work is exact-head CI, merge into `feat/rust-rewrite`, and a fresh branch.
Native order-history ID/status options explicitly reject before dispatch;
the previously documented measurement-gated and unavailable endpoint limits
remain unchanged.

### GRVT landing integration

Merged landing `5d7589f296`, retaining both ports' registrations, replay profiles,
CI rows and gate annotations. Incoming `baselineBeforeSetup` applies before
verifier setup in both runners; the reviewed detached comparison snapshot still
runs before async memory settling in both paths. Independent integration review
found no conflicting state or transport behavior. Harness 405/405 passes. The
new measurement lifecycle requires fresh public/private and Binance regression
measurements; prior measurements above predate this integration.

### CI follow-up correctness findings

The integrated public gate missed its unchanged steady-CPU threshold (TS/Rust
11.6/12.2 ms). Independently inspecting the actual public recording also found
68 gateway subscription-rate errors: the unpaced startup burst left most ticker
subscriptions inactive. Paired native/reference pacing and recovery, direct
native decimal decoding, fresh live recording and new release gates are underway.
The public-only harness no longer loads an unused private signature verifier;
private signature verification and all public request/socket validation remain.

Phemex CI repeated the 1300/1280 balance mismatch before the capture boundary.
Its TS polling and terminal reads could overwrite newer private pushes. Paired
revision checks now also fence disconnect/reopen and overlapping REST commits;
revision advances before any accepted REST store update. Independent review
found and closed those two additional authority gaps. Twelve deferred-read and
lifecycle regressions, the complete 63-test Phemex suite, typecheck and lint pass.
No fixture changes, balance normalization or performance threshold changes.

The subscription fix now passes independent code-correctness re-review. Both
implementations use bounded per-socket FIFO subscription pacing at 150 ms,
independent pings, and rebuild current intents on gateway rate rejection.
Review also caught TS `ReconnectingWebSocket.close()` disabling recovery;
using shared reconnect plus an idempotent suspended state fixes it. Three TS
proofs cover 78 paced subscriptions, stale callbacks, and duplicate rate errors
through actual shared wrapper recovery. Native tests cover numeric decoding,
FIFO/unsubscribe, both error forms, old-timer cancellation, queue bounds and
independent heartbeats. No parser success/failure semantics were relaxed.

A new actual live recording captures 20 seconds after market readiness plus
startup, 607 retained records, 525 WS frames, 417 ticker updates, 78 subscription
acknowledgements and zero rate errors. Both requested books arrive. Trim retains
all records (32-second physical envelope includes startup); nothing is added.
Combined public gate passes three runs: TS/Rust CPU 143.8/109.4 ms (1.31x),
steady CPU 32.8/28.4 ms (1.16x), memory 17.1/2.2 MB (7.91x).
The private userAssetData event now follows its paced subscription at 600 ms,
chronologically after the 400 ms command; frame/command/write counts unchanged.
Binance combined public/private gates passed after the GRVT lifecycle change:
CPU 266.4/137.7 ms (1.93x), memory 17.9/4.8 MB (3.72x), three books.

### BreakoutProp landing integration

Merged landing `74b0a7dd14` after it advanced during final CI. Preserve polling
transport readiness/timelines alongside TxFlow's explicit private virtual-clock
opt-in and startup freeze; both runners still capture comparisons before memory
collection and retain the incoming setup baseline. Keep recorder session policy,
HTTP counters and TxFlow's explicit public CORS egress; both roster/feature/CI
entries survive. Independent staged integration review accepts the union.

Immediately before this integration: TxFlow 71 native tests, harness 405, host
65 and real-Wasm wrapper 105 pass; strict native/combined/isolated Wasm Clippy,
formatting and bans pass. TxFlow-only Wasm is 893,268 bytes. Root tests are being
rerun after the merge so modules and newly added polling tests share one settled
source snapshot. Refreshed combined TxFlow public/private and fixed Phemex
private gates pass; merged harness/build/replay checks remain before landing.

### Final public decoder optimization

CI's only remaining replay failure missed steady CPU by 0.1 ms despite passing
parity, total CPU and memory. Decode channel-first public frames and typed
payloads in one pass, instead of validating a RawValue envelope and parsing the
same payload again. Numeric instrument lookup now uses active-only metadata
without allocating a decimal string; subscription acknowledgements skip their
ignored content directly. Cold fallback preserves reordered RawValue candles,
control/unknown envelopes and both outer/nested gateway error codes. Contexts
and books still validate completely before any state update. Independent
correctness review found and closed an outer-code rate-error edge case.

All 73 native tests, strict Clippy, formatting and diff checks pass. Isolated
release public gate, three runs: PASS, 527 replay frames; TS/Rust CPU
137.4/86.3 ms (1.59x), steady CPU 46.0/26.1 ms (1.76x), retained memory
17.1/1.3 MB (12.73x), linear Wasm 0.5 MB. Private 35-frame gate passes.
Single-venue Wasm is 943,476 bytes (budget 1,258,291). A second Cargo input check
is up-to-date and produces the identical artifact hash, proving the final ACK
variant was included. No fixture, threshold or reference change in this phase.
Root tests on the merged source pass 7,388 tests in each safe-cex run; typecheck,
lint, harness 437, host 65 and real-Wasm wrapper 105 pass. Final combined artifact
regressions and green exact-head CI remain before merge.

### Binance variants landing integration

Merged landing `e391337054` (Binance variants and GRVT profiling handoff).
Retain incoming coin-native balance projections inside the detached TS snapshot,
including `tsBalanceCurrencies`, while preserving both snapshot/measurement
boundaries. All registration, unit-conversion and session audit changes survive;
inverse conversion is inactive for TxFlow/Phemex. Independent integration review
accepts the union. Merged harness 455/455 passes. No TxFlow fixture or runtime
behavior changes in the merge.

The isolated decoder gate passes, but the subsequent default-feature public gate
missed steady CPU (TS/Rust 28.1/29.4 ms), with total CPU and memory passing.
Do not treat earlier passing timings as proof of this combined artifact.
Profiling one canonical replay is underway before choosing another optimization;
keep every comparison, audit and threshold intact. Final landing still requires
both applicable local gates and green exact-head CI.

### Profiled replay overhead

One canonical combined-artifact diagnostic replay identified shared benchmark
work rather than another guessed venue change. The 525 WS messages contain
257,620 bytes and no Bitget `arg`. Sampling attributed 22 self ticks to the
unconditional Bitget-only JSON parse, 36 inclusive ticks to repeated URL
canonicalization under socket selection, versus 26 inclusive handler samples.
These whole-process samples and diagnostic timings are not gate measurements
or a before/after speedup claim.

Guard the unused `arg` parse by the existing Bitget selector, and reuse the
already-computed canonical socket key. Each active-socket check still reads the
live map/set and applies the same closed/ordinal/stream/subscription/matcher
checks; nothing caches selected sockets across waiters or reconnects. All
Bitget routing and every delivery/audit hook remain intact. Independent review
accepts the change; all 455 harness tests pass, including existing fanout and
lifecycle proofs. No fixture or gate threshold changes. Final performance and
correctness gates are still required on the resulting combined artifact.

### Final local acceptance

All local gates pass on the integrated implementation after the profiled replay
fix. Final isolated three-run public gate: 527 replay frames, 78 markets/tickers,
two books; TS/Rust CPU 76.1/57.1 ms (1.33x), steady CPU 12.2/9.4 ms (1.30x),
retained memory 17.2/1.3 MB (12.97x), linear Wasm 0.5 MB. Wasm is 943,476 bytes
against 1,258,291. Isolated private correctness passes all 35 frames.

Final combined three-run public gate also passes: CPU 81.3/54.9 ms (1.48x),
steady 12.9/8.9 ms (1.45x), memory 17.2/2.1 MB (7.99x). Binance public passes
873 frames/three books: CPU 193.2/88.5 ms (2.18x), memory 17.9/4.8 MB (3.72x).
Bitget public passes 39,457 frames: CPU 644.7/538.9 ms (1.20x), memory
20.8/5.4 MB (3.85x). Combined TxFlow/Binance/Phemex/Bitget private correctness
passes 35/14/14/12 frames without new output normalization.

Native TxFlow 73 and engine 67 tests, bans, workspace formatting, strict venue
and combined/isolated Wasm Clippy pass. Root typecheck/lint/tests pass (44/28/27
tasks; safe-cex 7,396 tests in each run). Harness 455, host 65, real-Wasm wrapper
110 pass. All independent reviews and accepted fixes are posted on PR #1329.
Only exact-head CI, normal merge into feat/rust-rewrite, and a fresh branch
remain. Existing testnet and measurement-gated feature limits still apply.

### Frame server and consumer-readiness integration

Merge landing a7caec9314, preserving the incoming shared command decoder,
native frame server, optional book deltas and consumer-readiness suites. Keep
TxFlow's dependency/feature/constructor union; no venue behavior changes.
The real-venue fixture now explicitly selects TxFlow testnet and provides a
test-only public CORS route. Only the captured-input adapter unwraps that exact
route after production routing; real socket delivery, native ticker frames and
late-store equality remain required. Update the current cutover inventory to
44 registered Rust names, nine defaults, 35 opt-in names and five absent names.

Independent code-correctness review accepts the union and bounded fixture/docs
changes. Targeted combined-Wasm TxFlow consumers pass six tests in two files;
the full 68-test host suite passes. Engine 75, TxFlow 73, native server/transport,
cross-language frame-wire, harness 455, bans, workspace formatting and combined
Wasm Clippy pass. Full root and consumer suites, release replay/size gates and
exact-head CI are being repeated on this integration before merge.

### Live-smoke integration and paced consumer assertion

Merge landing 0063068241's credentialed lifecycle runner. TxFlow has no
reviewed authenticated live-smoke profile; it continues to refuse all modes
before credential access. Add it to both explicit unreviewed-profile test
inventories, preserving that production guard. This resolves the shared CI
inventory failure without authorizing or performing authenticated live writes.
Independent review accepts the harness union and finds no protocol change.

The full consumer matrix exposed a test race: any initial socket send did not
prove that all paced private subscriptions had been sent. Await all three
actual node wallet subscriptions and verify their exact types and wallet.
The two targeted real-Wasm tests and explicit lint pass; independent review
accepts this stronger assertion. Root typecheck/lint/test gates also pass on
the frame integration (7,408 safe-cex tests in each root test run).
