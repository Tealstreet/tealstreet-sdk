# CLAUDE.md - safe-cex Package

The Rust-only beta build admits only `src/exchanges/rust/betaVenues.ts` names.
SDK factories and workers have no TypeScript venue fallback; OUT names refuse
with `unsupported` / `notSent` before I/O. Standalone signing codecs, metadata
and test-only replay/reference implementations remain independent dependencies.
Historical Rust-adapter fixtures mock only beta admission through
`__tests__/reference/retainedRustPolicy.ts`; unmocked construction tests enforce
the real refusal boundary. These fixtures confer no beta activation.
Hosted-enabled beta names are Binance x4, Bybit x4 and HyroTrader, OKX x4,
Bitget x3 (USDT, USDC, inverse), Bitunix (#2262), Phemex, BloFin, BingX, and
Hyperliquid/Spot, Lighter/Robinhood and Paradex (remoteSigner only). Provider-mode eligibility still follows each declared remote
policy, with no separate beta override.
Rust option-query capabilities remain false until the loaded native record
explicitly enables them; a generic wrapper method alone never advertises them.

Core exchange integration library. See also exchange-specific `CLAUDE.md` files in `src/exchanges/*/`.

> **CLAUDE.md is a living spec.** When you change non-obvious
> behavior here or in any per-exchange CLAUDE.md, update the relevant
> doc in the same commit as the code change. See the root
> `CLAUDE.md` → "RULE — CLAUDE.md files MUST be kept current" for
> the directive.

For exchange migrations that add first-class opening trigger/conditional orders,
read `CONDITIONAL_ORDERS.md` before changing mapper or order lifecycle code.

## Mirror CI invariants (don't "clean these up")

safe-cex ships in the `tealstreet-sdk` OSS mirror, whose CI does a clean
sliced install with **no dependency hoisting and no leaked DOM lib** (see
`mirrors/README.md`). Two things look removable but are load-bearing:

- **`tsconfig.json` keeps `dom` in `lib`.** The package uses DOM/WebWorker
  globals (`MessagePort`, `Worker`, `PromiseRejectionEvent` in
  `src/webworker/store.webworker.ts`, plus `CompressionFormat` /
  `BufferSource` from imported `utils`/`reconnecting-websocket`). In the
  monorepo these leak in transitively even with `lib: [esnext]`; the mirror
  has no leak, so dropping `dom` breaks mirror typecheck.
- **Every imported package is declared in `package.json`** — including
  `jsonwebtoken` (used by `accessToken.ts` / `proxyTokenClient.ts`). It
  resolves via hoisting in the monorepo but must be explicit for the mirror.

## Commands (run from `packages/safe-cex`)

```bash
yarn lint          # ESLint
yarn typecheck     # TypeScript check
yarn test-ci       # Run all tests (deterministic, no network)
yarn test          # Watch mode
```

Rust venue release profiles live in `sdk-rs/Cargo.toml`; standalone overrides
live in `sdk-rs/harness/build.sh`. Size profiles must pass the fixed family and
combined budgets, public CPU/memory gates and complete raw replay comparison,
retaining identities and financial fields. Native release builds share the
package setting; measured performance evidence covers WASM.

Migrated Rust signing uses the shared dispatch-time Signer (`docs/plans/RUST_SIGNER.md`).
The construction-rules `dispatchSigning` flag selects provider setup, currently
Binance's four aliases, Bybit's five aliases, WEEX (hosted admission disabled),
AsterDEX and Lighter. AsterDEX first maps its account through `asterdexSignerSetup.ts`
(resolved API version, hex V3 wallet keys, read agent aliasing the primary
wallet); native setup and `asterdex-signer-setup.golden.json` pin the same
mapping. High-level credential input is consumed by a trusted
`LocalSignerSetup` before `BaseExchange` retains options; generic options contain
no primary/secondary signing credentials. The WASM constructor's optional third
argument is private provider setup, separate from engine-options JSON. Cached
construction metadata preserves `canSign`/public mode; transport receives only
read-identity presence; managed routing uses explicit credential intent before
credentials are materialised, while legacy requests retain header-based routing.
Lighter's trusted native/JSON mapping uses a typed API-index principal when a
private Schnorr signing ref exists and the original key is not Nonblank.
Original SetupFacts stay unchanged; the principal is never presentable. Validated
public L1 lookup metadata is captured before stripping credentials. The resolved
Account/API index and installed signing ref bind the transaction/auth recipes.
Schnorr activation and eager-holder removal ship together in the venue cutover.

Hyperliquid local trading setup maps only the agent scalar to a trusted
ScalarOnly primary, with required secp256k1Digest and byte-exact hex input.
The public key/applicationId/vault account fields stay separate. SDK and native
setup share the Rust decoder: 0x and 0X prefixes are accepted, while surrounding
space, NEL and BOM refuse. The legacy SDK's 0X refusal is deliberately widened;
invalid input stays redacted. Native local trading uses the installed, engine-sealed identity and managed
Prepare/Bind/Claim for each serial physical call.

Hosted Hyperliquid never sends a bare digest. Each L1 action's sign work is
`[sha256(statement), publicAddress, sign secp256k1Digest]`; the statement is
`{kind: l1, action, nonce, vaultAddress?, isMainnet}` (`sign/statement.rs`).
The keyless provider sends it as ws-auth `hyperliquid-l1`, and the device's
WASM `hostedWalletSigning` export (`registry/src/hosted_wallet.rs`, feature
`hosted-wallet`, kept apart from the HMAC capture codec for the family wasm
budget) re-derives the digest with the venue's own code, refusing non-trading
actions and another network or vault. It signs only with the local provider. Startup binds the agent
address once through `hyperliquid-address` (answered only for the signed-in
account); the engine refuses any signature not recovering to it. PayloadSigner
stays refused. The private provider capsule omits the trusted trimSigning false
default for feature-off decoders, while policy-derived true remains explicit
and caller trimming metadata remains refused. Legacy top-level HL passphrases
are consumed and ignored before provider construction; explicit provider
passphrases and secondary setup remain refused.

Paradex local signing maps the account `key` into a trusted ScalarOnly
`starkDigest` primary through `paradexSignerSetup.ts`; the capsule is exactly
`{primary: {secret: key}}`. Rust setup reads a 66-character key as the Stark
scalar (the venue's existing parse) and derives any other from an Ethereum key on
the network's fixed chain, then seals the provider's public key to that provider.
`paradexAddress`/`subkeyAddress` stay public options; legacy `secret` and
`passphrase` are ignored. `/auth` and every write are signed at dispatch and fully
verified against the sealed key. Keyless `remoteSigner` Paradex (key = the Paradex
account address) binds the device's Stark public key at setup (`paradex-public`)
and sends each signing as one closed `paradex/1` descriptor (`paradex-stark`):
the device's `hostedWalletSigning` (`hosted_wallet.rs`) rebuilds and hashes it at
its own clock, and the engine verifies every returned `r || s` against the bound
key and its own hash. PayloadSigner is refused.

Rust `localSigner` setup metadata selects explicit secret encoding (`utf8`,
`base64`, `hex`, or PEM envelope), credential trimming and caller-assigned
`stableId` per primary/secondary identity. It is consumed with passphrases through
the private setup channel, never retained in generic exchange options. Unmigrated
Rust venues reject this metadata. Stable ids never default to key hashes or slot
names; identity/nonce consumers require the caller's intended stable mapping.

Trusted signer setup uses shared declarative per-family/per-slot normalization
from construction-rules.json. Unspecified fields preserve caller trimCredential
metadata; MEXC forces primary credential trimming and secondary preservation. Bitunix declares `trimSigning` with `trimCredential` for both slots, so the
signing key is trimmed once at setup too (applies once Bitunix adopts dispatch signing).
The same field policy declares primary required-for-private passphrases and
required secondary passphrases with primary fallback for OKX and Bitget aliases
and WEEX.
Missing/JSON-null secondary passphrases inherit an opaque primary reference;
explicit empty never inherits. Public mode bypasses only the primary requirement,
while secondary validation stays active. Shared publicMode intent takes precedence
over legacy publicDataOnly, including null/omitted requested. This declaration does
not enable signing for an unmigrated family.
Normalization and required-material validation occur before opaque identities are installed, and SDK cached
public/private intent uses the same effective presence before raw options are
stripped. Whitespace follows Rust Unicode White_Space (NEL removed, BOM kept),
not JavaScript String.trim. Secret/passphrase bytes and stable IDs are unchanged.
This policy does not enable an unmigrated family; opaque external providers
remain responsible for compliant provisioning.

The same construction declaration carries hosted signature encoding and an
optional passphrase presentation header. Registry HTTP captures snapshot that
policy and request a distinct Present operation for the setup-owned passphrase
ref; they never interpolate it into the HMAC payload. Formatting validates the
entire result and combined header bounds before publication. Callback precedence
uses the existing declared, case-insensitive header policy. These codec metadata
fields do not enable runtime family admission or replace actual hosted tests.
Callback setup reuses the same required-field/absent-only fallback resolver,
consuming passphrase material into a separate provider Present map and exposing
only opaque refs to the engine. Presenting one requires admitted signing work
for its owning identity; it cannot be used in a Credential payload part. Missing
required material rejects synchronously. The unchanged remote wire supplies only
the MAC, in the declared hex or canonical padded base64 encoding of 32 raw bytes;
it cannot supply a missing passphrase. Generic options and callback receiver
snapshots retain no passphrase, and disposal revokes the setup wrapper.

Callers must provision worker/hosted providers separately rather than copy
`exchange.options` as credentials, and use readiness/public-mode metadata instead
of checking `options.key`. Raw legacy native/WASM input is a trusted compatibility
adapter until those cutover callers migrate; it converges on the same provider
path and must be removed before TS venue/reference deletion.

For families with enabled hosted-signing admission, browser-facing `signHttpRequest`
and `signWsAuth` use the same owned provider through bounded signing-only captures, without
starting account discovery or venue transport work. The default synchronized
clock can still initialize its existing time-source calibration. The original HTTP config/WS response
wire stays intact. Shared standalone Binance/Bybit and AsterDEX V1 (an explicit,
setup-resolved `apiVersion: v1` only; never V3 wallet keys) request-shape interceptors,
their codec/site tables and the capture utility are the TS retained for remote
signing after TS venue classes are deleted; no TS venue/API factory is created.
They retain real qs/Axios serialization and caller-owned WS preimages. Capture
tokens are single use and fenced by raw deadlines, protocol expiry and clock
revision; provider text and partial results are not returned on failure.
A native `dispatchSigning` checkpoint alone does not enable these hosted APIs;
its family must also pass exact legacy-wire and actual WASM/SDK acceptance gates.
The BingX native checkpoint keeps hosted admission disabled until those gates.
Phemex admits RemoteSigner (hex HMAC, no PayloadSigner); its hosted REST codec
in `rust/legacySigning.ts` signs `path ‖ query ‖ expiry ‖ body` as the reference did.
Trusted setup removes both local credential slots from generic options; its
REST and user.auth recipes use provider custody and the signed expiry-only receipt.
The shared delegated-provider companion admits keyless `remoteSigner` and
`PayloadSigner` accounts only for aliases enabled in construction-rules.json.
RemoteSigner wins and cannot fall back to PayloadSigner/local secrets; secondary
remote signing is refused. PayloadSigner retains REST-only private feeds and
declared native result-header precedence. Its ordinary-function receiver is a
private frozen public-metadata snapshot with the selected public key, excluding
all secret/passphrase fields and arbitrary nested caller objects. Callbacks must
hold their own secret instead of reading `this.secret`; generic exchange options
retain neither credentials nor callbacks. Provision callbacks in the executing
host/worker because ordinary worker initialization cannot serialize functions.
The central policy also declares legacy WS labels and extraData codecs. Metadata
comes from prepared receipt/payload structure, never a new wall clock or advisory
context; HTTP remains distinct even when its query matches WS preimage grammar.
Signing-only HTTP captures accept only their trusted declared header policy;
WS and credential captures retain default deny.
The delegated provider initializes UTF-8 codecs only when provider work begins.
Importing the native registry before its host installs TextEncoder/TextDecoder
must remain supported; signing and frame I/O require those runtime codecs.

The beta registry always selects Rust in SDK factories and `ExchangeWorker`;
`ExchangeOptions.engine` remains an accepted compatibility input only.
`src/__tests__/cutover/*.wasm.test.ts` verifies the worker frame/store consumer
contract for every `RUST_VENUES` name; actual native startup and independent
binary lifecycle fixtures have separate evidence scopes. See phase 9 of
`docs/plans/RUST_REWRITE.md` for deletion blockers; no TS venue may be removed
merely because its name is registered in Rust.

## Exchange event listeners

`BaseExchange` uses a local TinyEmitter subclass for both TS and Rust venues.
One throwing listener cannot starve later listeners for the same event; errors
are reported through `console.error` with venue, event and original cause.
Reporting emits no error/log event, so failures in those listeners cannot recurse.
The inherited once/off/context and subscription-snapshot behavior stays intact.

## Rust public-feed ownership

Rust accounts use `publicFeed: { mode: 'direct' | 'disabled' }` or
`publicFeed: { mode: 'shared', port }`. Without an explicit policy,
`privateOnly: true` selects disabled; otherwise direct. The pre-start
`disablePublicWebsockets()` hook also reaches native suppression. Policies are
fixed before startup. Native suppression covers Public-role sockets and retries;
Private/Business roles remain independent even on the same URL. Metadata REST
and account reconciliation remain native to each account.

Rust private startup resolves once the native catalogue, balance, orders and
positions have loaded, so a retrying public ticker seed cannot time out and
dispose a healthy account. Public-only startup still requires native tickers.
Shared accounts additionally wait for compatible provider metadata and a real
matching ticker seed; private readiness never bypasses that validation. Venue
account mappers own any price prerequisites for authoritative valuation, and
`loaded.tickers` remains false until the native ticker seed actually succeeds.

Start a public-only `RustEngineExchange`, then construct `RustPublicFeedProvider`
(exported by the SDK). Call `provider.attach(channel.port1)` once per account,
and explicitly transfer `channel.port2` with its shared-policy options to that
account's worker. One provider per source owns book/candle subscriptions and aggregates ticker
interest from attached accounts; late book subscribers receive the native
snapshot without waiting for a new venue event. Tick data, books
and candles enter the receiving native engine by symbol and leave through its
normal frames; never copy them into only the JS store. Providers send no private
sections or account metadata. A running/loaded direct source is mandatory. exchange-server's
`externalPublicData: { type: 'in-process', source }` uses the same protocol over an
in-memory port (`serveInProcessPublicData`). One hub per source keeps a single store
subscription and broadcasts changed tickers every 250 ms. Like TS, it is lenient:
private startup continues after 30 s without a loaded source, account markets the
source has not listed are tolerated, and book/candle requests for them wait. A real
field mismatch still fails. Testnet accounts ignore the venue-keyed source and run
their own feed. Websocket gateways still reject.
A source may forward frames: provider-owned book/candle listeners still run
before the buffer is transferred, while ordinary worker callbacks stay silent.
Matching shared ticker seeds wake startup even when identical REST data means
the native engine emits no changed frame.
Book/candle listeners registered before startup retain current demand and replay
after native metadata is ready, including when the JS store was restored from
cache. Callbacks register before the first subscription, and forwarded catalog
frames precede any synchronous subscription data. Public `fetchMarkets` and
`fetchTickers` start an unstarted Rust engine and wait up to 30 seconds for their
native readiness flags, independently of private balance/order reconciliation.
Running reads return native snapshots; they do not force a metadata refresh.
The caller still owns the exchange after successful `validateAccount`, including
disposal; validation does not invalidate an instance intended for reuse.
Venue/network/product identity and native instrument IDs/flags/contract sizes
must match before account readiness; account-local metadata may be a subset.
Catalog changes require restarting provider/accounts. Heartbeats release dead
ports; provider loss or incompatible data disposes the receiving account without
a direct-feed fallback. Cached provider fields stay authoritative across later
account-local REST ticker polls; private state remains independent.

The TS `externalPublicData` option remains for TS adapters. Rust rejects it with
migration guidance to the explicit port policy. Existing gateway/application
owners must adopt this API deliberately; a Rust registry entry does not migrate
their configuration. The policy controls live feed ownership, not routing,
credentials, supported venue products, or whether explicit public REST queries
are allowed.

Rust `fetchOrders`, `fetchPositions`, `fetchBalance` and `fetchPositionMode` wait
for the relevant native readiness flags on cold starts, then apply and forward
the current native account snapshot. This closes the store-flush delay without
requesting a network resync. Public-only engines with no requested private
readiness reject once metadata is ready; explicitly loaded native empties remain
valid. Missing account-snapshot support, terminal engines
and disposal reject instead of returning cached rows. Synchronous in-process
decisions can call `syncAccountState()` once native metadata is ready; the Base
implementation is a harmless no-op for TS adapters. Native account reads carry
an explicit frame marker so both worker and main preserve observation freshness
counters; ordinary stream frames still advance them. Recursive notification
readers reuse the already-applied state rather than publishing another snapshot.

Rust `nukePositions` sizes one current native position snapshot with the shared
native-unit sizing helper, preserving each hedge leg. It submits one reduce-only
market batch and propagates partial or unknown outcomes without automatic retry.
`nuke` also attempts the venue's cancel-all operation after a close failure;
the original close failure remains the result. Concurrent nukes are rejected.

## Rust account construction

`exchanges/rust/construction-rules.json` declares signing and private-data
eligibility for every Rust account. TS reads it synchronously before WASM loads;
Rust embeds the same table and resolves it before credential validation and
startup. The optional `publicMode` object carries raw caller intent (including
remote versus payload signer and index presence) beside `publicDataOnly`.
Without it, legacy native/harness options keep their original interpretation.
WASM `construction()` reports credential-free `canSign`, `publicDataOnly`, and
Aster API-version metadata. `resolveConstruction` exposes the same resolution
without engine construction or I/O. CI compares both evaluators for every
registered name. Explicit-private Paradex validation belongs to the registry.

The table's compact `bridge` policy also resolves spot/demo identity, primary-only
read credentials, payload-signing transport, API-index validation and isolated
book subscriptions before WASM loads. Native `construction().bridge` reports the
same choices; runtime behavior uses the engine capability record. Do not add a
venue-name fallback for either path. FrameApplier quantity units continue to
come from MARKETS flags; native sizing computations remain unchanged.

The bridge forwards raw Hyro mode and production/simulator URL maps; the
registry selects network endpoints. Native callers can retain explicit
`endpoints` when the raw `hyrotrader` config is absent. Private non-Paradex
`payloadSigner` refusal also belongs to the registry. WASM constructor errors
carry JSON `venueError` detail, which the bridge wraps as `ExchangeError` to
preserve `unsupported`/`notSent` through startup and account validation.

## Rust engine capabilities

Each Rust venue reports a capability record (`safe_cex_engine::Capabilities`,
read through `EngineHost.capabilities()`): account identity, client-order
lookup, history axes, fills by order id, native cancel-all, leverage reads and
maximum, account-wide leverage and margin type, symbol-routed cancel/amend,
order-book queries and options. Clock selection is shared engine-host policy:
the default SDK transport awaits the shared `virtualClock` startup and reads it
dynamically for every venue. Explicit host/recording transports retain their own
clock; native servers supply their OS clock and own its synchronization. There is no per-venue
`hostClock` capability (wire bit14 is reserved). `RustEngineExchange` routes from that record,
never from venue names, and adopts it once the engine exists (`ensureHost` or
`start`); `engineCapabilities` is undefined before then. `leverageQuery` decides
`fetchLeverageForSymbol`: `onRead` asks the venue, `settings` answers from
published rails, and `onMetadata` answers from those rails once a metadata
refresh has loaded them but asks the venue for a symbol not yet refreshed (the
worker only asks when the active symbol has no rails). `onReadAndMetadata`
keeps explicit reads fresh and also refreshes a selected symbol through
`updateSymbolMetadata`; it does not add metadata reads to ordinary `onRead`
venues. WEEX uses `onReadAndMetadata` so selecting a flat symbol refreshes
its signed configuration despite cached rails; public-only metadata stays inert.
Missing history axes
take the branded base fallbacks, so `exchange.capabilities` reports them absent
once the engine exists. `account` also gates the engine: `FetchAccount` to a
venue whose record says `unsupported` is refused before the venue runs, so a
venue answering identity for only some products (Binance USD-M/PM, Bybit
except HyroTrader) reports it per product.

Journal adapters, closed-position formats, history-bound normalization, filter
refusals, STP/execution controls, numeric validation and book callback isolation
also use engine policies. Capability flags and enum fields append to the existing
two-word snapshot record without changing quantity units or older discriminants.

Capability-dependent calls made during startup wait for that existing startup
before validating or dispatching. Internal startup commands do not wait on
themselves, and ready accounts retain immediate placement/cancel-all dispatch.
With no capability record and no pending startup, or after failed startup, calls
reject with a method-specific `ExchangeError` (`invalidInput`, `notSent`) and
exactly-once reporting. A queued write that never dispatched remains `notSent`
even if startup traps. Worker forwarding still runs before local policy checks;
never start a second mirror engine to obtain capabilities. Explicit worker-aware
readiness is recommended before local history/preflight work; cached markets
alone do not establish it. Callers must handle typed readiness failures.

## Rust main-thread trading

The web builds a second `RustEngineExchange` on the main thread; the worker's
instance owns the engine. An instance with a `callMethod` listener (the host's
worker bridge) forwards the `WORKER_FORWARDED_METHODS` in `exchanges/base.ts`
(trading, nuke, client-ID lookup, public trades, candles, history axes; plus
`fetchHistoricalPositions` and the native account reads `fetchOrders`,
`fetchPositions`, `fetchBalance` and `fetchPositionMode` by name, since those
wait on native state and would otherwise start a local engine; the worker
answers them as `WORKER_OPTIONAL_METHODS`, and a mirror's `syncAccountState` is
a no-op because worker frames keep its store current) whole through
`callMethodInWorker`, so preflight, placement scopes and nuke retries run
against the worker's state and report there once. This holds even if a
main-thread read (`fetchMarkets` during warm start) started a local engine; the
worker never has that listener. Without a bridge, an instance runs locally.
`updateOrder` crosses as `{ orderIds, clientOrderId }` because array properties
do not survive a JSON bridge. Interrupted forwarded writes carry
`outcome: 'unknown'` and the caller's client order IDs (`WORKER_WRITE_METHODS`,
`workerCallClientOrderIds`); requests still buffered when a worker stops before
startup are answered `notSent`. Cancel-all and nuke wait five command deadlines.

A bridged mirror never builds an engine for capabilities. In frame mode it
adopts the record a snapshot frame carries; in store-payload mode (and from
`loadCapabilities`) it asks the worker (`fetchEngineCapabilities`) once the
mirror store reports markets loaded. Capability-defined methods
(Lighter `changeAccountTier`/`fetchAccountTier`, history fallbacks) then exist on
the mirror, and the tier methods forward to the worker
(`WORKER_OPTIONAL_METHODS`). A failed load retries after 30 seconds. Branded
capability fallbacks are never wrapped, so `capabilities` keeps reporting them.

## Rust engine failure

Physical socket/request/signing/timer IDs are exact JS-safe integers through
`2^53 - 1`; command IDs and durations retain their existing bounds. Host adapters
must not coerce resource IDs to 32 bits. WASM rejects malformed numeric IDs before
parsing payloads or advancing clocks. Checked allocator exhaustion is sticky and
uses the same terminal path as a trap, withholding the complete queued output
batch. Workers retire their host and flush intervals before reporting terminal
StartupFailed; captured pending write failures still reach their original owner
across replacement, while stale successes and diagnostics remain suppressed.
Native account streams settle pending owners Internal/Unknown and replay only the
terminal notice, without constructing a frame from the failed engine.

A native WASM trap makes that exchange instance terminal (`isEngineDead`),
settles its pending commands and emits `engine-dead` after one visible error.
The host stops its I/O and ignores late callbacks. Ordinary venue validation,
transport and consumer-callback errors do not mark the engine dead. Interrupted
writes report an unknown outcome with supplied client order IDs; never replay
them automatically or interpret failure as proof that no order exists.

A running ExchangeWorker makes at most one delayed reconstruction attempt with
a fresh exchange and native state, retaining only public subscription demand.
The new stream starts with a full native empty snapshot to discard old book and
catalog baselines. Chasers/TWAPs stop and commands arriving during recovery fail.
Shared-feed accounts require a fresh provider attachment and account init;
their closed transferred port cannot be reused. Module-wide WASM initialization
and sibling accounts remain independent of one account's terminal state.

A terminal `StartupFailed` rejects `start()` with an `ExchangeError` that keeps
the engine's `kind` and `venueCode`; `validateAccount` returns the same `kind`
beside `error` (the typed failure wins over the error log venues emit just
before it), so callers can branch on `authInvalid`/`authPermission` instead of
matching text. A bare deactivation carries no `kind`.

Rust command waits default to 120 seconds. `extra.rustCommandTimeoutMs` sets a
positive integer deadline up to 2147482647 ms; long paginated queries may need
a larger value. The Rust worker RPC allows one additional second for delivery;
TS worker RPCs retain their five-second wait. Expiry does not cancel the venue
action, dispose the engine or retry a write. `COMMAND_TIMEOUT` and
`ENGINE_DISPOSED` preserve unknown outcomes for interrupted writes and currently
known caller client IDs. Native-generated identity receipts are tracked by the
bridge-fixes plan; caller IDs alone do not establish complete attribution.
Native rejections retain known partial-order results. SDK worker and outer CLI
executor error boundaries copy an explicit allowlist of command fields, including IDs and
partial counts, instead of flattening them or copying arbitrary error objects.
Cancellation commands retain the caller's id, symbol and type alongside legacy
ID dispatch; venue unknown-ID behavior is a separate native contract.

Rust command rejections report once at the SDK command boundary. Explicitly
correlated native diagnostics remain warning logs while pending or late; a
known partial success still emits its first associated error. Background logs
retain their normal visibility. Reporting preserves the original rejection and
all outcome/receipt fields even when listeners throw. The shared command-error
allowlist carries `errorReported`; frozen errors use weak object metadata rather
than message matching or a retained operation ledger. Subscription commands own
their rejected promises without reporting the same failure a second time.

Definitive GRVT builder approval refusals expose stable
`GRVT_BUILDER_UNAUTHORIZED`, `reason`, `requiredRaw` and optional `authorizedRaw`.
The SDK-root `builderAuthorizationOf(error)` validates the complete record and
works for TS classes, native host errors and worker/hosted deserialization.
Use it instead of class identity or text. Unknown/pending/network/malformed
checks stay ordinary failures. The command error allowlist retains only validated
approval fields; the old class export stays available without prototype repair.

## Rust incomplete history

A successful partial Rust history read returns the available rows with a
command-attributed `history.incomplete` notice. The JS host marks
that command's array with non-enumerable `incomplete: true`, independently of
notice-level filtering. Absence of this positive assertion never proves that a
venue's history endpoint is exhaustive.

`CallMethodHandler` carries optional `data.completeness: { incomplete: true }`
beside the existing successful reply's `requestId` and `result`, through both
structured clone and hosted JSON. `BaseExchange.callMethodInWorker` restores
that pending request's marker before resolution and observers. Upgrade sender
and receiver together. Public `historyCompletenessOf`, `withHistoryCompleteness`
and `copyHistoryCompleteness` helpers preserve it before row transformations;
Rust derived position and reported closed-PnL mappings retain it even on empty
results. Bybit's derived history combines both source axes' assertions.

The journal fold rejects flagged fills and symbol-scoped income; with complete
account-wide fills it discards flagged income enrichment and leaves funding
unattributed. NX ingestion must still inspect completeness before transforms
and leave historical coverage uncompleted. Raw native/DO clients correlate the
translated event command themselves; this SDK transport does not change their
wire protocol. See B11 in `docs/plans/RUST_CUTOVER_CALLERS.md`.

## Rust socket health

Engine venues own their sockets and reconnect policy; the wasm host only
observes them. `EngineHost.socketHealth()` keeps per-channel (public, business,
private) open/first-frame/close state, at one property write per frame.
`RustEngineExchange` exposes one `RustSocketHandle` per channel through the
`BaseWebSocket` fields, so `getAllSockets`, `reconnect`, `recoverAfterOffline`,
CLI `ConnectionMonitor` and the base `dispose` work unchanged. It writes
`store.socketHealth` through the shared `summarizeSocketHealth`, on socket
changes and every 5 s, never through the TS watchdog. A private socket is healthy
for 60 s of silence, the engine venues' own idle limit. Reconnecting closes only
open sockets; a connect in flight is left alone, as in TS. `Deactivate` and
`auth.rejected` events raise `authFailureStreak`; a private stream delivering
again resets it.

## Client order ID contract

`PlaceOrderOpts.clientOrderId` is a caller-supplied ID for the venue-native
client-order-ID field. **Honored verbatim on exactly one native order, or
rejected before any write — never dropped, rewritten, prefixed or replaced by a
generated ID.** A silently substituted ID makes a later lookup by that ID
truthfully answer "not found" while the order is live, and the natural response
to "not found" is to place it again: a double placement manufactured inside the
SDK.

- **One mechanism, in `BaseExchange`** (`exchanges/clientOrderId.ts` holds the
  types). A venue that can honor the field declares `clientOrderIdRule` (label,
  charset/length check, optional shape override); a venue without one rejects
  every caller ID. `resolveCallerClientOrderId(opts)` returns the ID or rejects
  through `rejectPreflight` (emit, then throw). It rejects non-regular shapes
  (anything but plain Limit/Market with no trigger, stop-loss or take-profit),
  an ID the venue rule refuses, and an order whose amount exceeds the market's
  `maxSize` (it would split). Call it as the **first statement** of
  `placeOrder`; `assertCallerClientOrderIds(orders)` is the `placeOrders`
  equivalent and also refuses a duplicated ID, so **a batch is all-or-nothing**:
  one unhonorable ID rejects it before any order in it is sent. Formatters then
  set the native field from `resolveCallerClientOrderId` and call
  `assertSingleNativeClientOrderId(id, payloads.length)` as a backstop.
- **Every override of `placeOrder`/`placeOrders` must call the guard**,
  including venues that honor nothing. `client-order-id.fail-closed.test.ts`
  drives every venue class with every transport replaced by a recorder; add a
  new venue to its tables. That test found HyroTrader's own `placeOrders`
  override placing the first order of a batch before the second one's ID was
  checked.
- **Honoring venues** (rule in each venue's CLAUDE.md): Binance USD-M/PM/Spot/Options,
  OKX (+EU/demo), Bybit linear/inverse/spot (+demo), Bitget (+inverse/USDC),
  KuCoin futures/spot, Phemex, BloFin, BingX, WEEX, Bitunix, Bitrue, Backpack,
  Coinbase (+spot), Crypto.com, WOO, Orderly family, Paradex, GRVT, ApeX Omni,
  Hyperliquid (+spot), TxFlow (testnet plain Limit/Market). **Hyperliquid honors every shape**, stops and brackets
  included (`isHonorableShape`): a stop is one native order, and on a bracket
  the caller's ID goes on the entry while each TP/SL leg gets its own minted
  cloid. It also mints a cloid for every order it places, caller ID or not.
  **Rejecting venues**: AsterDEX, BreakoutProp, HyroTrader, Kraken (+spot),
  Lighter, MEXC, Polymarket.
- **No broker prefix is composed onto a caller ID.** The generated paths keep
  their broker-attributed IDs (`x-s3vFFRQp…`, OKX/BloFin `BROKER_ID…`, Phemex
  `BROKER_ID_…`); a caller ID goes out bare. **To keep attribution, get the ID
  from `mintClientOrderId(opts)`**, which returns one in the venue's own
  generated format (`ClientOrderIdRule.mint`) or `undefined` where the venue
  would refuse one — it never emits, so call it before deciding how to send. A
  venue whose attribution lives in its ID must declare `mint`, or minted orders
  silently lose it; the fail-closed test pins the prefixed venues. Per-venue
  behaviour, including where the ID lands on a mapped `Order`:
  [`CLIENT_ORDER_IDS.md`](CLIENT_ORDER_IDS.md).
- **`placeOrderWithClientOrderId(opts)` returns `{ orderIds, clientOrderId }`**,
  where `clientOrderId` is the value on the wire. It is the caller's string
  because no adapter transforms an honored ID; the numeric venues (Backpack
  uint32, WOO safe integer, GRVT uint64) accept only canonical decimal strings,
  so `String(wire) === clientOrderId`. `placeOrder` still returns venue order
  IDs only — except where the venue's canonical ID already is the client ID
  (GRVT, Binance PM).
- **`findOrderByClientOrderId(symbol, id)` finds an order whose placement
  response was lost.** A miss carries `definitive`, which is true only when the
  venue was asked about closed orders too; never treat a non-definitive miss as
  "not placed". See [`CLIENT_ORDER_IDS.md`](CLIENT_ORDER_IDS.md) → Lookup.
- **A cancel+replace `updateOrder` may resolve to `ReplacementOrderIds`** —
  still a `string[]`, plus `clientOrderId`, the generated ID the replacement
  went out with (`replacementClientOrderIdOf`). Hyperliquid does this, because
  its positional TP/SL replacement answers with no order ID at all.
- **`updateOrder` never moves a caller ID to a different native order.** A
  native amend keeps the venue order and so its client ID (Bybit `REPLACE_ORDER`,
  OKX amend, Crypto.com, KuCoin Spot, WOO, Orderly, Paradex, BingX, Bitunix,
  Coinbase). A cancel+replace path gives the replacement a GENERATED ID: reuse
  collides with venue uniqueness rules (Bybit/OKX/Bitget reject it, Binance Spot
  refuses it until the old order fills, Coinbase dedupes and returns the
  cancelled order without creating one), and `Order` carries no record of
  which client IDs were caller-supplied. The caller's ID stays on the cancelled
  order, so a lookup reports it cancelled rather than "not found". **Bitget's
  native modify-order sends `newClientOid: uuid()`**, so a Bitget amend does
  rewrite the client ID; changing that needs a live check of whether Bitget
  accepts the old ID there, because a rejection silently falls back to
  cancel+replace.

## Automatic retries are for reads only

**Never let `axios-retry` resend a write.** A connection reset after the venue
accepted an order loses the response, not the order; resending it places the
order twice. Every factory's `retryCondition` goes through
`utils/retryReadsOnly.ts`: `networkErrorOnRead()` where reads are GET,
`networkErrorOnRead(readsAndPosted(...paths))` where a few reads are POSTed
(Kraken spot's private API, KuCoin ws tokens, listen keys), or a venue predicate
where every request is a POST (`hyperliquid/hyperliquidRetry.ts`,
`grvt/grvtRetry.ts`). `onlyReads(cond)` wraps a broader condition (asterdex's
503/504 rule); a signature-error retry (bybit, bitrue, apexomni) may stay, since
the venue rejected that request. WEEX and Crypto.com enforce the same rule
themselves. `__tests__/audit-regressions/no-retried-writes.test.ts` drives the
real factories with a connection reset, and fails if a factory imports
`axios-retry` without the helper.

The Rust transport uses owning `ReadRequest`/`WriteRequest` classifications.
`RetryPolicy` is constructible only from an immutable read token and cannot
serve as a general `RestAdapter`; signature-rejection retries remain separate.
Custom read predicates cover versioned listen-key suffixes. Conditions receive
the request as well as the result; optional host-timed delay takes the one-based
retry count, with zero delay by default and linear seconds for Asterdex.
The Rust retry-doctest gate removes each specific invariant independently and
rejects unrelated-error substitutions; `compile_fail` error-code annotations
do not pin the failure reason on Rust 1.99.

## Fault bench — proving behaviour on a bad link

`src/__tests__/faultBench/` scripts what a link does to each request: deliver,
drop before send, **lose the response of a request the venue acted on**, time
out, hold a reply in flight (`hold`), add latency, or reject without acting.
`createFakeVenue(routes)` records every request that reached it, so "sent once"
and "the venue has it" are assertions, not inferences. `bench.install(xhr)`
sits beneath remote signing, so a real venue factory runs its own signing,
interceptors and axios-retry over it — `badLink.scenarios.test.ts` drives
`OKXExchange` this way. **Key faults by route (`bench.on('post /path', ...)`)
whenever requests run concurrently**; an unkeyed fault goes to whichever request
reaches the adapter first. A known gap is an `it.fails` that names its phase of
`docs/plans/BAD_LINK_RESILIENCE.md`; it fails loudly the day a fix lands, and is
then flipped to `it`.

## Test placement and trust boundary

`src/exchanges/**` is production runtime code only. The legacy tests, manual
probes, and debug scripts in that tree were removed in July 2026 because they
were untrusted integration-generation scaffolding. Do not restore, copy, or
migrate assertions and fixtures from that deleted corpus.

Fresh exchange audit regressions live in
`src/__tests__/audit-regressions/`. Derive each oracle from current official
upstream specifications, verified audit evidence, or an explicit runtime
contract documented in this CLAUDE.md tree. Tests committed to the package must
be deterministic and must not require network access, API keys, or local secret
files. Live probes belong outside the test runner and outside `src/exchanges/`.

The shared Vitest setup denies and records unstubbed fetch, XHR send and socket
connections. Cleanup rejects recorded attempts even when exchange code catches
the immediate error. Supply explicit local fixture facts instead of relying on
an endpoint response. An intentional in-process socket fixture must bind loopback
and use the test guard's exact-port allowance, revoked in `finally`; there is no
blanket localhost or exchange-host exception. See
`docs/plans/TS_UNIT_TRANSPORT_ISOLATION.md` for the test-only boundary.

Vitest discovers deterministic `*.test.*` files throughout `src/` and excludes
only explicitly named `*.integration.test.*` and `*.network.test.*` suites.
It does not blanket-exclude `src/exchanges/**`, and coverage includes that tree
so reports expose the real production-code coverage floor. Keep the runtime tree
test-free; the absence of an exchange-tree exclusion is intentional and prevents
future test files from being silently ignored.

## Reconciliation fetch and store safety

**A final order event goes through `store.markOrdersTerminal`**, not
`updateOrdersStatus`, when the venue's event is genuinely final (filled,
cancelled — never "triggered", never "expired" on a venue that expires stops as
they fire). It keeps a REST snapshot that read the venue before the event from
reopening the order. Opt-in per venue; see `src/store/CLAUDE.md`.

`fetchOrders()`, `fetchPositions()`, and `fetchBalance()` return reconciliation
data. Callers must assign, return, or explicitly commit that data; a bare,
awaited-only, voided, or unused `Promise.all` fetch is not a refresh. The local
`safe-cex/no-discarded-fetch-result` ESLint rule enforces this in
`src/exchanges/**`. Keep fetch implementations pure: store mutation belongs in
the caller after disposal and stale-snapshot guards, so the same fetch can be
used safely by startup, tick, and WebSocket healing paths.

Hyperliquid is the documented narrow legacy exception: its DEX-aware order
reconciler performs surgical store mutation inside `fetchOrdersInternal()`.
Fire-and-forget callers use the explicitly named `refreshOrders()` wrapper; do
not generalize that exception or discard `fetchOrders()` directly.

Error paths must preserve last-known-good state. A failed reconciliation should
rethrow, or return the relevant current store value when best-effort behavior is
part of the venue contract. Never turn a transport/auth/rate-limit failure into
`[]` and then commit it as an authoritative empty snapshot. This convention is
documented rather than statically enforced because distinguishing a legitimate
successful empty response from an error fallback requires venue-specific
control-flow and response knowledge.

Rust diagnostics emit `engine-event` with the full `EngineEvent` (code, level,
message, fields), forwarded by the worker as `{ op: 'engine-event', data: [event] }`.
`ExchangeHealthMonitor.processEvent(event, classified?)` uses codes only, plus
the `kind` a classified notice carries in `event.fields` when no `classified`
is passed;
`processHealth(exchange.getHealth())` copies authoritative lifetime counters,
including diagnostics hidden by minimum-level filtering. Category counts and
recent errors count observed reports, separately from engine counters. Subscribe
to the structured channel once; skip legacy notifications only when they
carry `engineEvent`. Keep listeners for uncoded command/host errors.
For compatibility, Errors still emit `error` with `opts.engineEvent` and Info/Warn
still emit `log` with the event as the third argument. Telemetry has `event_code`.
This preserves existing auth badge/reload delivery until downstream cutover;
new Rust consumers use codes/typed errors, while legacy TS keeps text patterns.

The `'error'` event's options carry `classified: { kind, outcome, venueCode }`
for typed engine errors (`classifiedError`), and
`ExchangeHealthMonitor.processMessage(message, isError, classified)` lets
specific kinds (auth, rate limit, insufficient balance, venue unavailable,
network/timeout) win over text; catch-all kinds (`rejected`, `invalidInput`, `internal`, `unsupported`)
only replace the `unknown` fallback, because text patterns know venue auth codes
the engine leaves generic. Hosts must pass `opts.classified` from their `'error'`
listener into `processMessage` for this to apply.

`src/health/errorPatterns.ts` is not just toast labeling. The web controller
uses its `auth_invalid`, `auth_expired`, and `auth_permissions` categories to
pause automatic account reloads after terminal credential/permission failures.

There is a fourth terminal family, `startup_failed`, which this file cannot
produce and must not try to: it is keyed off the `worker_error_phase: 'startup'`
TAG that `ExchangeWorker.start()`'s catch attaches, because the string "Error
starting exchange" is the log message rather than `error.message` and so never
reaches `processMessage` at all. Do not add a pattern for it here — a venue
message that happens to mention starting up is not the same fact, and the
controller applies the tag only when this file produced no category.
When an exchange emits a known bad-key, expired-key, signature, or IP-allowlist
message, keep the health patterns aligned so it enters that terminal family.
Do not classify transient network, 5xx, timeout, or rate-limit messages as auth.

**And do not let an appended HTTP status do it for you.** `processMessage`
matches over the WHOLE message, so `appendHttpFailure` (`utils/httpFailure.ts`)
appends the status NUMBER and never the canonical reason phrase. Enumerating
which phrases are safe was tried and abandoned: three review rounds over one
small change found three separate collisions — `'unauthorized'` / `'forbidden'`
/ `'not allowed'` (401/403/405) push a working key into the terminal auth
family, `'server error'` (500) and `'timeout'` (408/504) demote a red toast to a
deduped log, and `/market.*not found/i` (404) is FIRST in the list under
`suppressed`, so it would drop the error with no toast at all. That last one is
a REGEX bridging the original message and the suffix, which substring reasoning
misses entirely. The list also grows whenever someone adds a pattern here, with
nothing to warn them the append exists. The bare number costs nothing: `'429'`,
`'502'` and `'503'` are already patterns, and their categories are the right
answers for a transport failure, so the number classifies exactly what is worth
classifying and is inert everywhere else.

## `instrument-class/` — what a contract is a claim on

Venues list tokenized equities, ETFs, metals and FX beside crypto perps, on the
same endpoints, with the same symbol shape: `SHOPUSDT` looks exactly like
`SOLUSDT` and does not trade like it. `classifyMarket(exchange, market)` answers
what an instrument IS, derived from `Market.info` alone so no venue needs a
request of its own.

Three properties are load-bearing, and each is the mirror of a rule in
Tealstreet's revenue attribution (outside this package), which this registry is modelled on:

- **`unknown` is not `crypto`.** A venue with no classifier must be loudly
  unclassified; defaulting it to crypto says "safe to trade like a perp", which
  is indistinguishable from an instrument we actually checked. Hyperliquid is
  the live case — its `meta` universe carries `name`, `szDecimals`,
  `maxLeverage` and `marginTableId` and nothing about asset class, so "HL lists
  only crypto" is a claim about their roadmap, not their API.
- **It never answers "can I trade this".** Eligibility is the venue's product of
  instrument metadata, session calendars and per-account agreement state, and
  only the first is public. The honest sentence is "this is a tokenized equity,
  which may require an agreement you have not signed". Bybit's
  `/v5/user/agreement` SIGNS legal terms and must not be called from a passive
  check.
- **An unrecognised venue value is reported verbatim, not mapped to crypto.**

Bybit is implemented (see `exchanges/bybit/CLAUDE.md` for its `symbolType`
table and the `innovation`-is-crypto trap). Binance
(`contractType: 'TRADIFI_PERPETUAL'`, with Binance's own typo) and bingx (which
already has `isLikelyTradfiMarket` as a boolean heuristic) are the next two, and
their absence means unwritten, not "all crypto".

**Three surfaces consume it, through one copy helper (`describe.ts`):** the CLI
order preflight in `cli-core`'s `prepareCliOrder`, the CLI `check <symbol>`
command, and the web preflight in `apps/web/src/order/tradfiPreflight.ts`. All
three share `describeInstrument` for a one-line summary and
`tradfiPreflightMessage` for the refusal body — the "may require an agreement"
line is load-bearing and drift between terminal and browser is exactly how a
user acts on one wording and sees another.

## Risk, PnL-at-price, ROE and break-even — where each one actually lives

Four related calculations, at different stages of convergence. **Read this
before writing a new one**, because nothing else in the repo maps them and the
last person to need a break-even added a fifth without tripping over anything.

### Before you write anything here, read the measured evidence

**[`docs/POSITION_MODEL_VENUE_EVIDENCE_v1.md`](../../docs/POSITION_MODEL_VENUE_EVIDENCE_v1.md)**
measures what venues actually report — that the same trade is GROSS on the
fills axis and NET on the closed-positions axis, that bitunix's `realizedPNL`
is net of fees _and_ funding, that fee SIGNS differ between venues added
back to back. `docs/CLAUDE.md` says to read it "before designing anything that
folds fills into positions". That instruction was ignored on 2026-09-30 and
three venue claims were committed on the strength of a field's NAME; one was
already contradicted in that file.

**A claim about what a venue field MEANS needs a measurement or a citation.**
Not an inference from its name, and not "it is probably scoped to the
position". If neither exists, the honest code path is a refusal.

### The inverse rule — and what is NOT true about it

Inverse handling belongs in SIZING. `safe-cex/src/utils/nativePositionSize.ts`
branches on `isInverse` because an inverse contract is a fixed QUOTE amount, so
its count comes from the notional.

**What does NOT follow, and was wrongly asserted here:** that
`Position.contracts` being expressed in base makes `entryPrice * contracts` a
valid notional, and therefore that price/PnL code never needs an inverse
branch. Every inverse mapper derives `contracts` from the **MARK**, not the
entry — `bybitinverse.exchange.ts`, `bitgetinverse`, `bitmex` and `okx` all
compute `notionalUSD / (mark || last)` — so that product equals
`notional * entry / mark` and drifts with the market. It is only the notional
at `mark == entry`.

On top of that, **bybitinverse and bitgetinverse leave `unrealizedPnl`
base-denominated** (bitmex and okx convert; these two do not), so on those
venues the numerator of any return calculation is in the wrong currency.

That false rule was used to DELETE a correct refusal from the CLI's `$roe` on
2026-09-30. The refusal is back. Treat inverse as unsolved in the price/PnL
layer, and refuse rather than answer until someone measures it.

### 1. Risk / PnL-at-price — one engine, five bypasses

`utils/risk.ts` (`calculateOrdersRisk`, `calculateRiskAtPrice`,
`buildOrdersRiskMaps`) is the engine. `OrdersRiskProfilePostprocessor` writes
its output to `store.ordersRisk`, and the chart's `OrderLineManager`,
`RiskIndicator` and the module API read it.

Six other places compute PnL-at-price independently:
`apps/web/src/components/riskProfiler/riskProfiler.calculations.ts` (flagged as
a known blur in `packages/safe-cex/src/webworker/postProcessors/CLAUDE.md`),
`openPositions`' worst-case-loss column,
`apps/web/src/components/chart/broker/lineManagers/bracketUtils.ts`,
`order/createStopLossOrder.ts`, and tealchart's mobile bracket preview. The
`clickStopLoss` hotkey templates were listed here and contain no PnL
arithmetic at all.

**The worst-case-loss column is measurably wrong on inverse**
(`useOpenPositionsColumns.tsx`). Its generic line — `notional * pct` — is
already correct, because `notional` is USD for inverse; the `isInverse` branch
then OVERWRITES it with a formula expecting a USD contract count and is handed
base units. A $100k BTCUSD position stopped 10% out reports about $0.11
instead of $10,000, so the max-drawdown tag can never fire for an inverse
user. `order/createStopLossOrder.ts` carries the same reciprocal branch.

### 2. ROE / ROI — one definition, correctly shared

`utils/roeRoi/calculateReturns.ts` is it.
`script-api-core/src/utils/calculateReturns.ts` is a **re-export, not a copy** —
this is the pattern the rest of this section should converge on.

**There are TWO return functions and they are not interchangeable.**
`calculateReturns` uses the venue's `unrealizedPnl` over `entryPrice *
contracts`; `calculateRelativeReturns` uses `(exit - entry) / entry`. The OPEN
positions table renders the first, Closed and Recent render the second, and the
CLI's `$roe<N>` inverts the second while its docstring claimed the first. They
agree only where `unrealizedPnl == (mark - entry) * contracts`.

`calculateReturns` also has a truthiness bug: `overrideUPNL ? overrideUPNL :
position.unrealizedPnl` means an override of exactly `0` — which the 1m/5m PnL
columns produce whenever the baseline is absent — silently reports the whole
position's ROE instead of zero.

`roe = (upnl / (entryPrice * contracts)) * leverage * 100`. Note the divisor is
a LINEAR notional, which is why inverting it for an ROE-denominated price is
only defined on linear markets.

### 3. Native position sizing — converged, and the proof it works

`safe-cex/src/utils/nativePositionSize.ts` is the single definition;
`script-api-core/src/order/positionSizing.ts` re-exports it for existing callers.
Web's `getPositionNativeSize`, the CLI's `nativeCloseSize`, and Rust bulk closes
use it. Its own comment records that the copies HAD drifted
before they were merged. One function in the shared layer, adapters either
side.

### 4. Break-even — NOT trustworthy, and the order paths are off it

`script-api-core/src/order/breakevenPrice.ts` is the arithmetic, and the
arithmetic is the only part that is right. **Its input is wrong.**

`processTrades` emits `contracts` as `Math.max(totalBuy, totalSell)` — GROSS
traded volume, not the size still open — and `getBreakevenPrice` feeds that in
as the divisor for banked PnL. Buy 10 @ 100, sell 5 @ 110 banking $50: the true
break-even is 90, the emitted row gives 95. It is wrong on exactly the
scaled-out positions the number exists for.

Nothing consults `historical_coverage`, so a position whose open falls outside
the ingested window yields a confidently PARTIAL number rather than none. The
30-day backfill depth and the 500-row read cap both make that ordinary.

**So no order path uses it.** `breakevenAllPositions` places at `entryPrice`;
the CLI's `be` answers `entryPrice`. The chart's B/E line, the hidden column
and the hotkey template still read it — a wrong display is a bug, a wrong
order is worse — and the column now renders an em dash rather than `0.00` when
there is no row.

**`api.orders.setBreakeven` does not compute break-even.** It places a stop at
a price the caller supplies. `api.utils.getBreakevenPrice` is the calculation.

Known defects in `processTrades`, unfixed: hedge-mode shorts are seeded with
`abs()` where the reverse walk needs a negative, so no row is ever emitted for
them; a long/short flip leaves the CLOSED leg carrying `isOpen: true`, which
`getHistoricalPosition` then matches on; there is no fill dedupe, and ingestion
re-walks windows.

See `apps/web/src/positionsHistory/helpers/CLAUDE.md`.

## Metadata capability non-fallback baseline

`BaseExchange` deliberately supplies inert defaults for `start`, `placeOrder`,
and `fetchHistoricalFills`. These functions carry a private
runtime brand; derived class fields replace the branded function during
construction. `exchange.capabilities` is the only reader: a capability is
present when its method (`CAPABILITY_METHODS`) is neither the branded fallback
nor an unconditional `return []`. Subclasses opt out of an axis with
`markBaseCapabilityFallback`. The deterministic capability
suite in `src/__tests__/audit-regressions/exchange-metadata-capabilities.test.ts`
loads the real registry and enforces the following minimum contracts:

- trailing-stop, partial/full TP/SL, or trigger-type advertising requires a
  non-fallback `placeOrder` and `features.orders: true`;
- `supportsPublicOnly` requires construction with `publicDataOnly: true` and a
  non-fallback `start` implementation;
- TP/SL trigger defaults must belong to the unique advertised trigger set.

The fills-history runtime gate is deliberately not metadata. Ingestion checks
`exchange.capabilities.fillHistory` and then applies
`symbolScopedFills.ts` to decide whether the venue can be asked account-wide or
must be walked by symbols discovered from the income ledger. A branded fallback,
an unenumerable symbol-scoped venue, and an empty result are three different
states; do not collapse them into a capability boolean.

**There is ONE fills axis, and it is `fetchHistoricalFills`.** `fetchHistoricalTrades`
and the `supportsTradesHistory` flag were deleted 2026-08-16. They were never a
second capability: the two methods hit the SAME venue endpoints (binance both
`USER_TRADES`; bitrue's `fetchHistoricalFills` literally called
`fetchHistoricalTrades`), differing only in that one returned `HistoricalTrade`
with millisecond timestamps and the other `Execution` with seconds. Keeping both
bought two mappers per venue that drifted apart in silence — bitmex, asterdex,
weex and bitrue each dropped `realizedPnl`, hedge leg or maker on one side and
not the other. The mappers were made strict supersets before either method was
removed, so nothing was lost in the fold.

`supportsTradesHistory` went with it because it never gated anything: only test
fixtures read it, and it had drifted to advertising `true` on `phemex`,
`backpack` and `lighter` — all of which always returned empty — while advertising
`false` on `woo`, which worked. `supportsExecutionsHistory` was later removed for
the same reason: no UI or ingestion path consumes a metadata fills-history gate.
Do not reintroduce a second history method or a second capability manifest; if a
venue needs a different route, it belongs behind `fetchHistoricalFills`, not
beside it.

This baseline proves only that metadata capability flags still in use do not
point at inert base methods. It cannot prove that an override maps every order
type or trigger correctly, that a history endpoint returns valid rows, or that
public startup succeeds end-to-end. Those claims need venue-specific
deterministic request/response regressions; live startup and authentication
additionally need network or credentialed probes. Do not replace that evidence
with source-text grep.

## `payloadSigner` (Bybit, Binance)

`ExchangeOptions.payloadSigner` replaces `secret` for callers that hold no
secret and sign elsewhere. It receives the exact bytes the venue would HMAC
and returns the value for the signature slot, plus headers to add to the same
request. `utils/payloadSigner.ts` routes both modes through one function, so
local signing and delegated signing share the same payload. Binance and Bybit
skip the private websocket with this REST signer. No Rust venue consumes a
payload signer; the engine's payload-signing actions and events are removed. `runsOnRustEngine` keeps
Bybit- and Binance-family accounts with one on TS, whose signers honour it.
Every other TS venue, including the hosted Coinbase/Orderly classes, ignores a
payload signer while `canSign` counts it, so those accounts stay on the engine.
A Rust account that would run privately with a payload signer refuses at
startup, before any engine or transport work: an `ExchangeError` with
`kind: 'unsupported'`, `outcome: 'notSent'`, which `validateAccount` reports
with its kind. Lighter's payload-only accounts still start public. Tests: `src/__tests__/payload-signer.test.ts`,
`src/__tests__/paradex.payload-signer.test.ts` and
`src/__tests__/audit-regressions/rust.payload-signer-refusal.test.ts`.

## Account data on shared public markets (`loadAccountMarketOverlay`)

On exchange-server, a private account takes markets from a keyless shared
public exchange (`externalPublicData: in-process`), which cannot fetch anything
signed. Anything per-account that a venue's `fetchMarkets` folded in behind
`isPublicDataOnly` is therefore missing: Binance/BinancePM leverage brackets
(max was 1 everywhere), Bybit/BybitInverse fee rates, OKX EU's
`ACCOUNT_INSTRUMENTS` list. The fix is one mechanism in `BaseExchange`:

- `start()` calls `loadAccountMarketOverlay(load)` when
  `needsAccountMarketOverlay`, **before** `initExternalPublicData`. The hosted
  store delta compares only top-level primitives, so an overlay that lands after
  the first snapshot never reaches the client.
- Both public-data providers pass every market delivery through
  `adaptExternalMarkets`, so the overlay survives shared-exchange restarts.
- The overlay must return NEW objects: shared markets are referenced by every
  account on the venue.

**A new venue that puts signed data into `Market` must add an overlay**, or its
exchange-server users silently get the keyless value.

## Telemetry sink (`src/telemetry.ts`)

safe-cex reports to nothing by default. `setTelemetrySink({ event, captureException })`
is how a host opts in; the web main thread registers one in
`apps/web/src/utils/telemetry/safeCexTelemetrySink.ts` (imported from
`instrumentation-client.ts`) that forwards exceptions to Sentry and the
`hyperliquid.builder-fee-approved` event to `/api/logger/builder-fee`. **Never
import a telemetry client or POST to a Tealstreet route from this package** —
call `reportException` / `reportEvent` and let the host decide. Workers still
ship errors through the `sendToSentry` op, not the sink. `emitAndLogError` (the path every command rejection takes,
in the worker, main thread and exchange-server) and `ExchangeWorker.logError`
tag typed engine errors with `error_kind`, `error_category`, `error_outcome`
and `venue_code` (`exchangeErrorTags`, closed set only), so filters can key on
the kind rather than venue text.

## Proxy & Tunnel Architecture

Exchanges connect to APIs in three modes, controlled by metadata flags in `exchangeMetadata.ts`. The decision logic lives in `createExchangeFromAccount.ts` and request wiring in `safeCexAxiosCreate.ts`.

**All routing goes through a `TransportProvider` (`src/transport/`).** `safeCexAxiosCreate`
asks it for the REST baseURL and proxy authorization, every ws class resolves its host
through `resolveTransportRouting`, and `BaseWebSocket.createWebsocket` asks it how to
open a socket (relay, or routing headers). Factories get their tunnel adapter from
`installTransportAdapter`; one-off calls (coinbase/woo absolute URLs, builder
approvals, the public CORS proxy for clocks, UDF and lighter, the secondary-key and
public-server proxies) use `proxyPrefix` / `privateProxy` / `withPublicCorsProxy` /
`serverProxyUrl`. A method returning nothing means "go direct". Tealstreet's pools,
tunnel and passthroughs are `transport/tealstreet/tealstreetTransport.ts`.

**Engine hosts use the same provider.** `RustEngineExchange` wraps its raw host
transport with `transport/engine.ts`; the venue still emits real URLs. Resolve
the account's routing once and reuse provider REST authorization per operation.
HOW uses the package's raw adapter so signed request strings, large native IDs,
status/error bodies and response headers survive unchanged. Preserve the venue
factory gates (BingX private defaults to HOW unless explicitly disabled; Binance
public clients bypass ordinary proxies; WEEX/GRVT/BloFin do not install HOW).
KuCoin futures and spot install HOW for private clients even if
`tealstreetTunnel` is false or unset. Paradex payload signing skips its API
factory's HOW adapter; JWT acquisition uses a direct request in every mode.
Account normalization already applies metadata; do not normalize explicit
headless options again at this boundary. Engine socket effects carry optional
public/private/business roles; new venues with shared URLs must supply the role.
Reuse `WsDirectFallback` and the feature-gated package relay. Native launchers
serialize `resolveEngineRoutingPlan(exchange, options, realRestBaseURL)` and pass
it to `NetworkTransport::with_routing` with a fresh context token provider; never
serialize a JWT. API venue engines mirror the TS secondary client's GET-only
identity and egress when `secondary_key` and `secondary_secret` are supplied;
`secondary_passphrase` overrides the primary passphrase only when present.
Writes, posted reads and private socket authentication retain the primary key.
Definitive client-ID reads and Kraken's separate history client explicitly use
primary routing; Bitget degraded-stream balance repair retains its fast primary
client. Public GETs remain unsigned where the venue omits authentication; those
using the TS dynamic client still select its secondary egress. Binance's public
client and Aster's public API keep their existing routes. Incomplete secondary
credentials reject before I/O. Aster's separate read-agent auth contract remains
unchanged. HTTP `primary` hints fence primary-account reads from the secondary
route. Optional HTTP `direct` / `publicCors` route hints support venue clock fallback;
hosts consume these hints before network I/O. Public CORS routes come from the
provider (and serialized native plan), never venue URLs. Missing native CORS
policy fails explicitly; normal requests retain account routing and deadlines.

Managed engine HTTP uses `Transport.prepareHttp` to finish route/proxy-token
and tunnel readiness before signing. `credentialIntent` (`public`, `primary`,
`secondary`) selects the route independently of HTTP method or credential headers;
custom routing providers must honor it. A custom HTTP/tunnel adapter without the
prepared interface fails explicitly instead of invoking its legacy sender.
Preparation and binding never send HTTP. Binding preserves signed URL/query/body
bytes and rejects conflicting host headers; `ready()` rechecks the original
request deadline and resource, and `dispatch()` invokes fetch/WebSocket.send
synchronously after the engine release claim. Cancellation releases only that
request's prepared/bound resources. Opaque legacy requests share the executor but
retain their older routing and carry no managed release/outcome guarantee.

Engine venue requests use native authorization headers. On Coinbase and its
spot alias, an ordinary REST proxy owns `Authorization`, so the host moves the
venue JWT to `Coinbase-Authorization` before acquiring proxy authorization.
Direct and HOW requests retain the original `Authorization`. Native plans carry
this as optional per-route `headerAliases`; native code applies those aliases
before route authorization and has no Coinbase routing table. Alias conflicts
fail before network I/O. Plans contain header names, never credential values.

**The default is `directTransport` (`transport/direct.ts`).** Every Tealstreet entry
point imports `transport/registerTealstreetTransport` as a side effect: the web main
thread (through `apps/web/src/registerTealstreetRealm.ts`, which also registers the
telemetry sink and snapshot upload — any host that copies apps/web, like the RN app,
must import that one file), its store worker (`StoreWorkerEntry.ts`) and
exchange-server worker (`ExchangeServerWorker.ts`), the CLI, exchange-server,
candle-server, and the `scripts/keys` / `scripts/history-audit` CLIs. **A new realm that
constructs exchanges must add that import**, or its accounts go direct. Direct refuses
an account carrying `tealstreetProxy`/`tealstreetTunnel` (it throws in `rest()`), so a
missed registration fails loudly instead of tripping venue IP allowlists. Tests that
depend on Tealstreet routing import the registration themselves, after their
`vi.mock` calls; registering in `setupVitest.ts` loads the transport before those
mocks and breaks them. **`ExchangeOptions.transport` covers an exchange's own REST and sockets only**; standalone helpers with no instance in scope (builder approvals, the clocks, Lighter's base URL) use the registered default, so a host that proxies must call `setDefaultTransport`. **Never put a provider in `ExchangeOptions.transport` where options
cross a boundary**: the web app posts `exchange.options` to its worker through
structuredClone, which throws on a function, and exchange-server's `INIT_FIELDS`
allowlist silently drops it. Pinned by `__tests__/transport-adapter-selection.characterization.test.ts`
plus the two routing snapshots.

**Routing source of truth (`src/transport/tealstreet/routing/`).** Which actual _host_ a request/socket uses — the REST CORS pool (jp/asia/tiny), the HOW egress region, and each exchange's public/private/business WS host — is declared per-exchange in `transport/tealstreet/routing/routingTable.ts` and resolved by `resolveExchangeRouting(exchangeName, options)`. REST (`getProxyForAxios`) and every per-exchange `ws-*.ts` class read their host from the resolver instead of hardcoding `wss://fsjnpy143pbz.*` literals. To change where an exchange's traffic egresses, edit the table — not the ws class. The Rust direct-provider WS warning uses `console.warn` on wasm (including Workers) and stderr on native, once per exchange; host logging runs after releasing the dedup lock. A poisoned warning-cache mutex is recovered, so logging bookkeeping cannot prevent subsequent socket routing. Rust callers derive `RoutingOptions` with `From<&ExchangeOptions>` (or the owned conversion); the live routing oracle exercises this account boundary for every golden case. The conversion exhaustively destructures account fields without `..`, so adding a field requires an explicit routing decision. The Rust routing input uses the account model’s `ExchangeOptionsBybitSiteId` enum and rejects misspelled serialized site IDs before resolving a host. The site-id doctest uses plain `compile_fail`; the ignored Node/Rustc gate compiles that exact example against the actual Cargo library and requires an E0308 JSON diagnostic. An unrelated error or removed type guard fails this gate; rustdoc error-code annotations carry no authority. The routing golden matrix pins 24 flag/region/site combinations per exchange, including Brazil no-op cases, and also supplies the Rust routing generator. Behavior is locked by `src/transport/tealstreet/routing/__tests__/routing.equivalence.test.ts` + `src/transport-resolution.characterization.test.ts` (the snapshots must not move except for intentional changes). Variant exchanges that reuse a parent's ws class (bitgetinverse/usdc→bitget, coinbasespot→coinbase, bybit family) get their parent's routing because the table declares them with the same shared `WsGroup`. **Exception: okxeu shares OKX's ws classes but has its own `WsGroup`** — its private/business tunnels go to `okxeu/*` paths that terminate on the EEA entity's hosts (`wseea.okx.com`), because EEA credentials do not authenticate on the global `ws.okx.com`. A channel may also declare an optional `fallback` host (the athena Caddy passthroughs `bybit-proxy-ja` / `okx-proxy-ja`); the resolver attaches it **only to a DIRECT resolution**, and the class's `WsDirectFallback` (`utils/wsDirectFallback.ts`) dials it after two consecutive silent connects. Bitget's public classes still do the same job with older inline counters and `fallbackOnly`. A PRIVATE channel may additionally declare `relayUpstream`, which the resolver turns into a tunnel `rpc.relay` url (`relay` on `ResolvedWs`, `RelayWebSocket` in http-over-websocket, dialled through the PRIVATE proxy fleet so the socket egresses from a whitelisted IP). The class walks the chain direct → relay → athena, but only offers the relay while `transport/tealstreet/wsRelay` is on — pushed by the host app from the `WS_RELAY_PRIVATE` flag via `__workerConfig`, which safe-cex forwards to host handlers (`registerWorkerConfigHandler` in `store.webworker`; apps/web registers it in `StoreWorkerEntry.ts`), off by default. **Bitget is the exception**: it declares `relayWhenTunnelled`
(the relay rides a tunnelled route too) and offers it regardless of the flag,
because Bitget rejects a private login from an IP the key is not bound to
(40018) — see `exchanges/bitget/CLAUDE.md` → "Private WS egress". It also
declares a separate `privateUta` channel, resolved as the optional
`wsPrivateUta`. `createWebsocket` recognises a relay url (`isRelayUrl`) and hands `RelayWebSocket` to the reconnecting socket as its `WebSocket` option.

**One set of hosts since AWS_EXIT step 4: the browser build's, which the hosted exchange server shares.** Master (the Rust bundle) sends every tunnel, relay, private-pool and per-venue socket to the Vultr host `fsjnpy143pbz-tyo` — Tokyo for every region, so `PRIVATE_PROXY_JP/ASIA/GRVT_TINY` and `TOKYO/SINGAPORE_HOW_EGRESS` are each one value, and okx/okxeu/bitget/blofin/coinbase private sockets are relay urls (`BROWSER_WS_HOSTS` in `routingTable.ts`). The public CORS proxy (`PUBLIC_CORS_PROXY`) is the same Vultr host's `/api/public_proxy`: the edge Worker serves it without a JWT, from the tunnel hosts' own IPs and never the whitelisted fleet, even when a caller attaches its proxy token (Binance public reads do), so public-data weight stays off the IPs users trade through (`infra/tofu/CLAUDE.md` → trade tunnel). **The rust lanes must not be released until `public_cors_enabled` is on and both tunnel hosts carry the public CORS container**, or every public read 403s. `services/exchange-server` pinned the old AWS hosts at boot (`pinTealstreetEgressHosts`); the pin was deleted on master 2026-10-29 (the running containers keep it until the cutover deploy, AWS_EXIT step 4) so a hosted account egresses exactly like a browser one, and `services/exchange-server/src/egress.test.ts` fails if the server sets a host of its own again. **Read hosts at call time** — `tunnelEgressUrl`, `getPrivateProxyHost`, `getPublicCorsProxy`, `getRoutingTable` — because tests substitute them (`setTunnelEgressUrls`, `setRoutingWsHosts`) after modules load. **`WS_RELAY_PRIVATE` is not a kill switch for the relay any more**: for a proxied okx, okxeu, bitget or blofin account the relay url IS the channel's `host`, which the flag never gated (it gates only the `relay` candidate a direct account may fall back to). Turning the flag off leaves those sockets on the relay; the rollback is redeploying the previous build (or moving the user back to legacy).

**`howEgress` is the one derived field — it is NOT a literal in the table.** The HOW (REST-over-WS) tunnel egress region (jp/sg) is owned by the `@tealstreet/http-over-websocket` package, which is what actually picks the egress at request time. The routing table computes `howEgress` via `isTokyoEgress(name)` imported from `@tealstreet/http-over-websocket/src/egressRegion` (the single Tokyo-egress list), so the table never duplicates that list. The exception is the region-pinned families (bitmex always, bybit when tunnelling) whose api factories force the HOW adapter region from `proxyRegion`; those are declared `regionConditional`. The single-source lock — table's name-based egress must equal the adapter's runtime baseURL-based egress for every exchange — is enforced in `routing.equivalence.test.ts`. To move an exchange's HOW egress, edit the list in `egressRegion.ts`, not here.

Canonical saved account construction lives in
`src/accounts/buildExchangeAccountConfig.ts`, with routing normalization in
`src/accounts/normalizeExchangeAccountConfig.ts`. Web, CLI, key-audit, and
other "add API key" flows should build or validate new credentials through that
shared account contract before persisting/testing/passing the account to
`createExchangeFromAccount()`. Do not reimplement exchange credential fields or
`tealstreetProxy` / `proxyRegion` account defaults in app code.

Exchange API factories must install HOW and other transport adapters through
`setInnerAdapterForRemoteSigning(instance, adapter)`, never by assigning
`instance.defaults.adapter` directly. Hosted exchange-server instances keep the
remote-signing adapter as the outer layer and execute the exchange-selected
transport only after the browser returns a signed request. The returned signed
config is authoritative: if the browser signer omits `data` because an exchange
requires signed POST bodies to move into query params (Binance futures hedge
mode is the regression case), the adapter must not restore the original body or
the exchange will validate a different payload than the signature covered. A
trusted audit regression scans every `*.api.ts` factory for direct replacement.

The Rust HTTP signing adapter keeps its signer and inner adapter as in-process
capabilities, with host-supplied unique IDs and clock callbacks. `RestRequest`
carries params, the public-request tag, and signing duration to the inner adapter.
Its `serde_json` dependency enables `preserve_order`: signer-bound and signed JSON
bodies and params retain browser insertion order, including a signature appended
after signed fields. Cargo unifies this additive feature across the whole build
graph using that serde_json instance, not just this crate. Everywhere in that graph
`Map::remove` becomes `swap_remove`, which can reorder surviving keys; use
`shift_remove` when their order matters. The production removal in the secondary
options helper decodes into typed options without serializing entry order, so its
reordering is inert. Non-UTF8 request bodies fail before signing or transport.

Private-proxy REST header injection is Node/Bun-only. Browser workers also lack
`window`, so use `isNodeOrBunRuntime()` rather than `typeof window` when deciding
whether code may set browser-forbidden `Origin`, `Referer`, `User-Agent`, or
`sec-fetch-*` headers.

Before changing proxy/tunnel routing, the typed source of truth is the
`routing/` table + resolver, locked by `transport/tealstreet/routing/__tests__/routing.equivalence.test.ts`
and its golden-master snapshot (the always-current, machine-checked per-exchange
routing map). The repo-root `docs/proxy-routing-flow.html` is a manually rendered human-facing
**mirror** of the same web → safe-cex → HOW flow (it re-implements resolution in
vanilla JS for an interactive table) — useful for browsing, but trust the snapshot
over it. If routing behavior changes, update or delete that page in the same PR.

**Extension direct-connect (web-side override).** The web app can force an
account fully-direct (`tealstreetProxy=false`, `tealstreetTunnel=false`) when
the user opts in (`ExchangeAccountConfig.useExtensionDirect`) and the Tealstreet
browser extension is present (`window.__tealstreetExtension`). The decision is
web-only (`apps/web/src/utils/exchange/extensionTransport.ts`), passed as a
last-wins `ExchangeOptions` override — safe-cex stays app-agnostic and does not
read `window`. The extension's declarativeNetRequest rules supply CORS/headers
for the direct calls. Bybit only for now; no-op (proxy stays the fallback) when
the extension is absent.

**Secondary (read-only) key — split read/write transport.** When an account has
a `secondary_apiKey`, `createApiWithSecondaryKey.ts` routes all REST **GET (read)**
requests to a second axios client built from the `secondary_*` credentials, while
writes stay on the primary. That secondary client's transport is set by
`ExchangeOptions.secondaryProxyUrl`, so reads can later egress through a separate
proxy while writes keep the primary/whitelisted transport. The feature is
**default-off**: with **no** `secondary_apiKey`, `createApiWithSecondaryKey` returns
three references to the _same_ primary client (the read Proxy is never built), so
routing is **byte-identical** to before. Precedence for the secondary read transport:
`options.secondaryProxyUrl || otherOptions.proxyUrl || <jp default>`, applied as the
secondary client's `overrideProxyUrl`. `createExchangeFromAccount` sets
`secondaryProxyUrl` to `PUBLIC_CORS_PROXY` — the same
edge Worker `/api/public_proxy` host `getPublicCorsProxy()` returns (the tunnel hosts' own IPs, never the whitelisted fleet).

The Rust `SecondaryKeyClients` exposes the same get-only split, keeping generic
request calls on the primary. The raw secondary adapter is private, so its
execution surface is GET-only. Its default-off path shares the primary adapter;
independent FIFO limiter wrappers use host clocks and release cancelled waiters.

**A Vultr "read fleet" was built for this in June 2026 and abandoned 2026-08-20.**
Twenty HAProxy boxes (jp/sg) behind Cloudflare were meant to carry the public CORS
proxy and these secondary-key reads off AWS, gated by `ENABLE_AWS_PROXY_MIGRATION`.
The flag never left 0%, the fleet never carried production traffic, and the cost win
it targeted was captured another way (VPC peering + tunnel compression, July 2026).
The flag, both `READ_FLEET_*` hosts, `getReadFleetProxy.ts` and `restEgressRegion`
were all deleted; there is now exactly ONE public CORS host and no region split on
this path. Do not reintroduce a second host without a rollout plan.
Still **default-off**: with no `secondary_apiKey`, none of this is reached.

**Wiring is per-exchange, gated by `exchangeSupportsSecondaryKey()`.** Every HMAC
exchange builds its authed `this.xhr` via the base helper
`BaseExchange.createXhrWithSecondaryKey(opts, createApi, otherOptions)`
(`src/exchanges/base.ts`), which calls `createApiWithSecondaryKey` and stashes the
raw primary/secondary clients on `this._primaryXhr` / `this._secondaryXhr`. The set
that wires it MUST equal the set the UI offers it for — i.e. everything **not** in
`exchangesWithoutSecondaryKey` (`src/utils/exchangeSupportsSecondaryKey.ts`). That
exclusion list is the wallet / single-keypair venues (hyperliquid, paradex,
grvt, apexomni, lighter, polymarket, and the orderly ed25519 family
incl. `tealstreet-dex`) **plus** the session-login venue `breakoutprop` (DXtrade
session token) — there a shared per-session token is injected on every request so a
second client can't authenticate as a distinct key. A read-only "second API key" is
not a coherent concept for any of these.
When adding an exchange, wire the helper **and** reconcile that list.
`binancepm` is the one bespoke case: it has no single `this.xhr` (uses
`papiXhr`/`fapiXhr`), so `BinancePMExchange.constructor` calls `createApiWithSecondaryKey` for PAPI alone
and keeps its primary/secondary clients for validation; FAPI stays primary-only,
as its constructor comment says it is unused until a private FAPI read path exists.

**Only the helper-wired client splits reads.** A few exchanges also build an
`unlimitedXHR` (rate-limiter-bypass client for urgent order placement) that is **not**
routed through the helper, so any GET issued on it stays on the primary transport.
Today the only authed read on that path is bitrue's `fetchOrdersForSymbol`
(`unlimitedXHR.get`); everywhere else `unlimitedXHR` is writes-only and the bulk of
read volume (tick-loop `fetchPositions`/`fetchBalance`/`fetchOrders`) flows through
`this.xhr`, which does split. Wiring `unlimitedXHR` for 100% read coverage is a
deliberate follow-up, not in this pass.

**`validateAccount` caveat (opt-in path only).** Because `this.xhr.get()` routes to
the secondary key once one is set, an exchange's `validateAccount` (which is a GET)
validates the **read** key, not the primary write key, when a secondary key is
present. binance and binancepm validate **both** explicitly via
`_primaryXhr`/`_secondaryXhr`; the other HMAC exchanges validate only the read key
on the opt-in path (write key is exercised on first order). The default-off path
(no secondary key) is unchanged. Validating both keys everywhere is a deliberate
follow-up, not an oversight.

### Metadata Flags

| Flag              | Default | Purpose                                             |
| ----------------- | ------- | --------------------------------------------------- |
| `requiresCors`    | `false` | REST calls go through Tealstreet IP proxy servers   |
| `supportsTunnel`  | `false` | Exchange supports HTTP-over-WebSocket (HOW) adapter |
| `enforceWsTunnel` | `false` | Force HOW adapter on (overrides `supportsTunnel`)   |

### Other metadata flags

| Flag                         | Default | Purpose                                                               |
| ---------------------------- | ------- | --------------------------------------------------------------------- |
| `hiddenFromNewAccountPicker` | `false` | Hide from the add-API-key picker while keeping saved accounts running |
| `supportedOnMobile`          | `false` | Allow this exchange to run in the React Native mobile app             |
| `supportsSpotToggle`         | `false` | Venue serves spot + derivatives from one connection; offer the opt-in |
| `supportsOptionsToggle`      | `false` | Venue trades options in the derivatives account; offer the opt-in     |

`hiddenFromNewAccountPicker` is deliberately **not** `enabled: false`:
availability gates also decide whether a _saved_ account may register at
startup, so disabling a superseded venue stops every existing user account on
it from loading. Set on `hyperliquidspot`, now served by `hyperliquid`'s spot
toggle.

`supportedOnMobile` is the separate mobile allow-list. It must stay
default-off because mobile account sync/import paths can receive any web account;
the app should keep unsupported venues saved but inactive instead of constructing
their exchange workers. Use `isExchangeSupportedOnMobile()` rather than direct
truthiness checks so unknown legacy exchange names fail closed.

The certified mobile set is locked by `exchangeMetadata.test.ts`: Binance
(`binance`, `binancepm`, `binancespot`), Bitget, BloFin, Bybit (`bybit`,
`bybitdemo`, `bybitspot`, `bybitinverse`), Hyperliquid, Kraken, Lighter, and
OKX. Spread variants not in that list must pin `supportedOnMobile: false` so
they do not inherit mobile availability accidentally.

`supportsSpotToggle` drives `ExchangeAccountConfig.enableSpotMarkets` →
`ExchangeOptions.enableSpotMarkets` (forwarded in `createExchangeFromAccount`
only when the venue advertises the toggle). Default-off, and the exchange must
return **before** issuing any request when off, so opting out costs nothing.

`supportsOptionsToggle` does the same for `enableOptionMarkets`. Options join the
existing connection only where the venue keeps them in the same account as its
derivatives, so balance, margin and positions stay one view. A venue with a separate
options account gets its own exchange name instead. Option markets carry
`Market.option`; chains, expiries and account greeks come from `fetchOptionChain`,
`fetchOptionExpiries` and `fetchAccountGreeks`; check them with
`exchange.capabilities` (`optionChain`, `optionExpiries`, `accountGreeks`). Chains are fetched on demand and never enter the store, so the
perp ticker path is unchanged when options are on.

### Decision Flow

```
Is requiresCors true?
├── NO → Direct API calls (browser → exchange)
│   │
│   └── Is enforceWsTunnel true?
│       ├── YES → WS tunnel only, no CORS proxy
│       │   Examples: Hyperliquid, Paradex, KuCoin, Kraken,
│       │            AsterDEX, BreakoutProp, BitUnix, BybitDemo
│       └── NO → Fully direct, no proxy or tunnel
│           Examples: OKX, Binance, BinancePM, BinanceSpot,
│                     Bybit, BybitSpot, BybitInverse, Woo, WEEX
│
└── YES → tealstreetProxy = true (REST routed through IP proxy)
    │
    └── Is enforceWsTunnel true OR supportsTunnel not false?
        ├── YES → Proxy + Tunnel (REST via HOW adapter through proxy)
        │   Examples: BingX, Bitrue, Bitget, GRVT,
        │             ApexOmni, Phemex, Coinbase
        └── NO → Proxy only (REST via CORS proxy, no WS adapter)
            Examples: Crypto.com, Backpack, BloFin, Polymarket
```

### How It Works

1. **Direct** (`requiresCors: false`, no `enforceWsTunnel`): Browser makes REST calls directly to the exchange API. Works when the exchange sends CORS headers or the app runs in a true server context (Node/CLI).

2. **CORS Proxy only** (`requiresCors: true`, `supportsTunnel: false`): REST calls are routed through Tealstreet proxy servers that add CORS headers. The proxy IPs must be whitelisted on the exchange. No WebSocket adapter involved.

3. **WS Tunnel** (`enforceWsTunnel: true` or `supportsTunnel: true`): REST calls are wrapped into WebSocket frames using the HTTP-over-WebSocket (HOW) adapter (`axiosHttpOverWebsocketAdapter`). This routes REST through an existing WebSocket connection. Can be combined with CORS proxy or used standalone.

### Common Pitfalls

- **Do NOT set `enforceWsTunnel: true` without ensuring the exchange's tunnel infrastructure is deployed.** The WS tunnel requires server-side support (tradetunnel).
- **`supportsTunnel: false` in defaults**: Many exchanges inherit this. If adding `enforceWsTunnel: true` to a new exchange, you don't need to also set `supportsTunnel: true` — `enforceWsTunnel` takes precedence in `createExchangeFromAccount.ts`.
- **`requiresCors: true` needs `privateProxyIps`**: The proxy IPs are shown to users for IP whitelisting. Without them, users can't configure their API keys.
- **Runtime checks for proxy resolution**: treat browser main thread and WebWorker as browser runtimes. Do not use `typeof window === 'undefined'` alone to infer "server"; that breaks worker-local proxy routing.
- **Package-boundary discipline**: do not import proxy helpers from `apps/web` into `packages/safe-cex`. Shared routing logic lives in `packages/safe-cex/src/transport/tealstreet/routing/resolveExchangeRouting.ts` and `packages/safe-cex/src/transport/tealstreet/transportResolution.ts`.
- **Multi-tenant token contexts**: `accessToken.ts` supports a per-async-context
  proxy token provider used by exchange-server. Contextual tokens are checked
  before the browser shared-worker or CLI/global refresh path, but invalid,
  expired, empty, or throwing contextual providers must fall back to the normal
  module token path. Do not replace this with module-global `setProxyToken` in
  multi-user hosts. Rust engine transports capture the provider of the context
  that created the account and reinstate it around each proxy authorization
  (`withProxyTokenProvider`), because engine requests start from timers, sockets
  and shared feeds whose async context is not the account's. HOW requests and
  relay sockets read the token after awaits or on open, so they bind the lookup
  at request/socket start through `HOWConfig.bindProxyToken`
  (`bindProxyTokenLookup`); the engine creates relays inside the scope.
- **A proxied REST request never leaves without proxy authorization.** The
  browser SharedWorker and the CLI answer `''` while their token warms; the
  provider's `authorize` polls for up to `PROXY_TOKEN_WAIT_MS` (15 s), then
  throws `ProxyTokenUnavailableError` ("Tealstreet proxy authorization
  missing"). The edge's plain-text 401 (`Authorization header is missing` /
  `Invalid JWT`) reaches a Rust engine as a transport failure, after a token
  refresh, never as a 401: venues classify a 401 as a refused key
  (`auth.rejected`). The engine transport reports either failure once per
  outage through `createEngineTransport`'s `onProxyAuthorizationFailure`, which
  `RustEngineExchange` sends to `emitAndLogError`. A venue's own 401 passes through.
- **Static validation**: `exchangeMetadata.test.ts` has invariant tests that catch impossible configurations (e.g., `requiresCors + forbidCors`). Run `yarn test-ci` after changing metadata.
- **Unknown metadata fails closed**: `getExchangeMetadata` throws for runtime
  names outside the exhaustive registry. Never restore a venue-shaped fallback;
  it can apply the wrong transport and capability flags to corrupt saved data.
- **Non-browser tradetunnel callers must use the shared header-capable
  transport with a browser-like `User-Agent`.** CloudFront/WAF in front of
  `fsjnpy143pbz.{jp,asia}.tealstreet.io` (and Cloudflare in front of
  `fsjnpy143pbz-tyo`) can reject WebSocket upgrades before
  the Rust tradetunnel sees them. Browser clients send their own UA. Node/Bun
  clients (CLI, `yarn keys`, exchange-server) must use either
  `@tealstreet/http-over-websocket` for HoW REST or `base.ws.ts`'s Tealstreet
  URL branch for exchange WebSockets. Both attach `Origin:
https://preview.tealstreet.io` plus a browser-like `User-Agent`, and rely on
  vendored `ReconnectingWebSocket` to force HTTP/1.1 via `ALPNProtocols`. Do not
  use Bun native `WebSocket`, undici/tcp-websocket, or a hand-rolled socket for
  tradetunnel/HOW.
- **Only required WebSocket upgrade headers fail closed without `ws`.**
  Set `options.headersRequired: true` for header-authenticated sockets; then
  `BaseWebSocket` rejects a native/browser constructor that would silently drop
  auth. Static browser-identification headers and implicit Tealstreet routing
  headers are best effort, so browser/native fallback remains supported.
  Header-authenticated exchanges must handle the typed unavailable-transport
  error and retain their independent REST safety path where present.
- **The reconnecting-WebSocket path alias targets `thirdparty/`.** Keep
  `tsconfig.json` pointed at the vendored source so safe-cex tests and
  typechecks do not resolve a hoisted/stale workspace copy instead.

## Key Files

- `src/exchangeMetadata.ts` — All exchange configuration metadata
- `src/accounts/normalizeExchangeAccountConfig.ts` — Canonical saved-account proxy/region shaping
- `src/createExchangeFromAccount.ts` — Translates account config → exchange options (proxy/tunnel logic here)
- `src/index.ts` — `buildPublicExchangeOptions()` is the shared public-only
  routing helper used by `createPublicExchange()` and exchange-server shared
  public exchanges. Keep those paths aligned through this helper.
- `src/safeCexAxiosCreate.ts` — Axios instance creation, HOW adapter setup
- `src/httpOverWebsocket/` — Thin safe-cex compatibility re-exports and
  `configureHOW` wiring; the adapter implementation lives in
  `@tealstreet/http-over-websocket`
- `src/proxyTokenExpiry.ts` — The single hardened proxy-JWT expiry check
  re-exported by both `accessToken.ts` and `proxyTokenClient.ts`; keep malformed
  token handling and virtual-clock semantics centralized here.
- `src/transport/tealstreet/routing/resolveExchangeRouting.ts` + `src/transport/tealstreet/transportResolution.ts` — CORS proxy URL / transport resolver used by safe-cex REST clients (the old `src/utils/getPrivateProxy.ts` no longer exists)
- `src/types.ts` — Shared types (`ExchangeOptions`, `ExchangeAccountConfig`, etc.)
- `src/health/errorPatterns.ts` — Fuzzy health classification. Critical auth
  expiry requires key/token/session/signature context; never add bare lifecycle
  words such as `expired`, `timestamp`, or `clock` as global patterns.
- `src/exchanges/*/` — Per-exchange implementations

## REST request counters ride the store, like `socketHealth`

`store.requestStats` holds cumulative per-outcome REST counts
(`ok` / `rateLimit` / `auth` / `network` / `other`), incremented from the
response interceptors in `safeCexAxiosCreate.ts` — **both** factories
(`safeCexAxiosCreate` and `safeCexAxiosCreateNoProxy`), success and error.

The same two success interceptors also call `exchange.noteHttpResponse?.()`,
which records the most recent non-2xx that axios **resolved instead of
rejecting** — the HOW tunnel adapter never calls `settle()`, so on that path no
axios error is ever built and the status is lost before any error is emitted.
It is read once in `emitAndLogError` and merged into the telemetry tags only; it never
touches the store, the message text, or `errorPatterns` classification. See
`utils/httpFailureContext.ts`. **Call it optionally (`?.`)** — `exchange` is
duck-typed in several paths and in tests, and an interceptor that throws on a
missing method fails the REQUEST; an unguarded call broke 51 tests.

Three constraints, each load-bearing:

- **The store is the only channel out.** This package ships in the OSS mirror,
  so it must not import a telemetry client. Same rule as `socketHealth`; the
  consuming app reads the field and reports.
- **`recordRequestOutcome` must never `notify()`.** It runs on every request.
  The throttled store ship already carries the field, so notifying here would
  manufacture a store delta per request — which is the main-thread churn that
  made per-request logging too expensive to keep.
- **Classification is HTTP-status-only.** `health/errorPatterns.ts` is a linear
  regex sweep over ~20 pattern groups and lives on the main thread inside
  `ExchangeHealthMonitor`; it is both unreachable from a worker interceptor and
  far too expensive per request. `network` (no response at all) is deliberately
  distinct from `other` (venue answered non-2xx).

**`auth` is one bucket, and it cannot tell you which KIND of auth failure.**
This is the asymmetry to know about before reaching for it: the web
`exchangeAccountController` classifies auth failures richly
(`auth_invalid` / `auth_expired` / `auth_permissions`) but only in memory, to
decide whether to keep reloading an account — while `requestStats`, the only
thing that persists and aggregates across users, sees nothing but a 4xx status.
So "phemex is failing auth" is answerable and "phemex keys are being rejected
for scope rather than being expired" is not. A 2026-08 production hour landed
12 auth failures from a **single** user against 3 successes, and no query could
separate a dead credential from a signature bug.

If you need the finer signal, it has to be a **closed-set enum decided at the
interceptor** — never a venue message, and never a call into `errorPatterns.ts`.
The store is structured-cloned across the worker boundary _and_ persisted to
IndexedDB by `exchangeStoreCache`, so a free-text reason would carry venue error
strings (which embed client IPs and signed query strings) into both. Widening
the enum also changes the counter table's primary key, the record RPC's
signature, the route's row normalizer, and the rollup's `group by`.

Counters are monotonic per exchange-instance lifetime and are **not** reset on
reconnect — a counter that resets is indistinguishable from one that never
moved. Consumers diff against their own baseline and must treat a decrease as
"the instance restarted", not as a negative delta.

## Server-pushed polling slowdown (`tickIntervalOverride`)

`getTickInterval()` multiplies its base by `getTickIntervalMultiplier(exchange)`.
The host app derives the map from a per-user feature flag and pushes it through
`__workerConfig` as a typed setter — safe-cex never sees a flag name, only a
number.

- **The map is keyed by exchange, with a reserved `'*'` wildcard.** The flag
  engine folds rules to one boolean per user, so there is nothing per-venue to
  express today and the producer pushes the wildcard. It is a map rather than a
  scalar so a future per-venue signal needs no migration, and an exact key beats
  the wildcard. Do not "simplify" the producer into enumerating the user's
  accounts — that means importing the exchange controller into the module that
  builds its workers, which closes an import cycle, and it reads the account
  list before the account is registered — `_exchanges.set(...)` runs AFTER
  `createStoreWorker()`, so an enumeration returns `undefined` for the very
  account being created, ships an empty map, and never self-corrects because the
  re-push subscribes to flags rather than to accounts.
- **It can only ever slow polling down.** Values below 1 are neutralised and the
  map is capped. A lever that could also speed polling up is one bad flag value
  away from causing the incident it was reached for.
- **The multiplier applies to the BASE, before the auth-failure backoff**, so a
  slowed venue with a rejected key still tops out at
  `AUTH_FAILURE_MAX_TICK_INTERVAL` rather than multiplying past it.
- **It does not reach hosted (exchange-server) accounts, and the reason is
  earlier than it looks.** `createStoreWorker` returns the exchange-server port
  _before_ it reaches `applyWorkerConfig`, so the setter is never called on that
  path at all — this is not a question of which ops the server forwards.
  Independently, `ExchangeServerWorker`'s `INIT_FIELDS` omits `extra`, so the
  hosted server already ticks on raw metadata regardless. If rate limits show up
  in hosted mode the lever has to be built server-side; do not assume this one
  covers it.

The Rust polling helpers preserve wildcard fallback, finite-value clamping,
and replacement of the entire map. `get_tick_interval` takes the resolved base
and auth-failure streak so the multiplier precedes the same backoff cap.

Every Rust SDK account sends `setPollScale` before native startup and when the
multiplier or Base auth streak changes: its resolved Base `getTickInterval()`
over the metadata `tickInterval`. Each venue keeps its own reconcile cadence
(Bybit 5s, Kraken 5-10s, Paradex 10s), so the scale slows or speeds those
cadences rather than replacing them with the metadata tick. The resolved tick also
travels as `ceiling`, so a server multiplier or the auth-backoff cap bounds the
slowed cadence as before unless a venue's native cadence is already slower. Effective option
changes are checked at the existing store flush cadence. Infinity is the explicit
scale `null`; venue constructor options do not carry a second polling policy. Unchanged values are suppressed, failed delivery stays
eligible for retry, and disposal removes the multiplier listener. These controls
govern periodic reconciliation, not socket supervision or explicit reads. Base
auth-streak propagation alone does not prove classified native auth feedback.

## Debugging exchange REST traffic

REST routed through the HOW adapter appears as WS frames, not XHRs.
The exchange emits `grafana-http-request` / `grafana-http-response` /
`error` / `warning` events on `BaseExchange.emitter` — check your
consuming app's CLAUDE.md for the capture playbook.

## Worker Position Refresh Bridge

`BaseExchange.updatePositionsInWebworker()` emits the `updatePositions`
worker op handled by `src/webworker/handlers/UpdatePositionsHandler.ts`.
Account-wide refreshes (`symbols` omitted) must use
`store.addOrUpdatePositionsFull(positions)`, including empty arrays, so
positions omitted by the exchange are zeroed/cleared. Symbol-scoped refreshes
remain partial and use `addOrUpdatePositions(positions)` when rows are
returned. Do not replace this with a main-thread `fetchPositions()` + direct
store write from the web app; the worker owns live exchange state and its
`updateIds.positions` counter.

## Worker CLI Exec Result Bridge

`src/webworker/handlers/CommandHandler.ts` emits `exec-result` only when the
incoming `exec` payload includes a `reqId`. Shape:
`{ reqId, handled, success?, error?, data? }`, where `data.orderIds`
is used by the web CLI capture syntax. Preserve `success` / `error`:
`cli-core` commands commonly signal failures by returning
`CommandResult.success === false` instead of throwing, and the browser
runner depends on that bit for `retry`, blocking steps, and capture
writeback. For fire-and-forget worker commands with no `reqId`,
handled failures should still emit `exec-log` so the terminal displays
the error.

## Orderly Rust engine

Tealstreet DEX, KodiakFi, WOOFi, Mode, SalsaDEX and What use one Rust venue
with broker options. Production registries must not import the frozen replay
reference in `src/__tests__/reference/orderly`. ED25519 signatures cover exact
request bytes. The shared engine transport supports direct requests, proxies
and the built-in raw HTTP-over-WebSocket adapter (`supportsTunnel: true`).
Authorization may add intermediary headers but must not change the original
URL suffix, body, method or venue headers. Socket roles and feed identities
are declared by the venue; the shared host applies account routing policy.
Custom provider upgrade headers require a capable host and are checked before
engine construction. History `orderId`/`status` filters fail explicitly rather
than returning unrelated rows. See `sdk-rs/venues/orderly/README.md`
for native IDs, margin modes, caller IDs and socket capacity.

## Native credential polling feedback

Rust account-read notices carry the construction-time signer slot and read
scope, never credentials or response bodies. Bounded counters isolate primary
and secondary Account/Balance/Orders/Positions/Settings failures; only matching
accepted evidence clears a counter. Their maximum drives the existing Base
polling policy, including server slowdown and Infinity. Unknown, public,
unsigned and retry-intermediate results do not clear or create failures.
Coinbase currently uses primary JWT credentials only: saved read fields and
their proxy are dropped from effective options with a warning before startup.

MEXC trusted compatibility setup preserves its existing persisted account IDs:
`mexc:` plus the first12 SHA256 bytes of the raw Primary API key are supplied as
explicit Primary stableId before provider trimming. JS/native setup reject a
conflicting supplied mapping. Generic LocalSigner never derives stable IDs;
MEXC venue execution reads only installed metadata, never hashes credentials.
