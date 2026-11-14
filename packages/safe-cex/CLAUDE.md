# CLAUDE.md - safe-cex Package

The Rust-only beta build admits only `src/exchanges/rust/betaVenues.ts` names.
Hosted construction follows that full roster for public and private accounts.
Bitrue admission is restored by SDK #3502; #3317 dropped a live venue without
a product decision. Its existing signing and credential-placement checks remain.
Polymarket, Weex, TxFlow and the canonical `orderly` name remain OUT;
KodiakFi, Mode, What and WOOFi retain their admitted Orderly-family aliases.
Polymarket's native signing layer also refuses delegated
custody: its private user WebSocket needs the L2 API secret, not a signature. See
`sdk-rs/venues/polymarket/README.md`; callback payloads must never export it.
SDK factories and workers have no TypeScript venue fallback; OUT names refuse
with `unsupported` / `notSent` before I/O. Standalone signing codecs, metadata
and test-only replay/reference implementations remain independent dependencies.
Historical Rust-adapter fixtures mock both beta admission entry guards through
`__tests__/reference/retainedRustPolicy.ts`; unmocked construction tests enforce
the real refusal boundary. These fixtures confer no beta activation.
Hosted-enabled beta names are Binance x4, Bybit x4 and HyroTrader, OKX x4,
AsterDEX (V1 HMAC and V3 API-wallet RemoteSigner; V1 PayloadSigner REST only;
see `sdk-rs/venues/asterdex/README.md`),
MEXC (RemoteSigner and REST-only PayloadSigner; see `sdk-rs/venues/mexc/README.md`),
Kraken Futures (RemoteSigner SHA256/HMAC-SHA512 custody; see `sdk-rs/venues/kraken/README.md`),
Bitget x3 (USDT, USDC, inverse), Bitrue, Bitunix (#2262), Phemex, BloFin, BingX, Crypto.com,
Backpack (RemoteSigner Ed25519 custody; see `docs/plans/callers/backpack-hosted-signing.md`),
Coinbase/Spot (RemoteSigner JWT and REST-only PayloadSigner; `docs/plans/callers/coinbase-hosted-signing.md`),
GRVT (RemoteSigner only; `docs/plans/callers/grvt-hosted-signing.md`),
KuCoin futures and Spot (RemoteSigner batch custody; see `docs/plans/callers/kucoin-hosted-signing.md` and `docs/plans/callers/kucoinspot-hosted-signing.md`), and
Hyperliquid/Spot, Lighter/Robinhood, Paradex and WOO X (remoteSigner only). Provider-mode eligibility still follows each declared remote
policy, with no separate beta override. Refused `remoteSigner` accounts are told to
turn Hosted Server off to use their API key in the browser. Refused `payloadSigner`
accounts receive neutral delegated-signing wording. Both use the venue display
name and retain `unsupported` / `notSent` before I/O.
Robinhood is admitted through the existing Lighter family and remote-signing policy; its preview/production visibility flags are unchanged.
Rust option-query capabilities remain false until the loaded native record
explicitly enables them; a generic wrapper method alone never advertises them.

`exchangeMetadata.orderAmendment` declares public order-shape restrictions for UI
eligibility through `getOrderAmendmentError`. BingX permits unfilled GTC/PostOnly
limit price or size edits, preserves attached protection on size edits only, and
permits SL/TP price edits. Native reconciliation and wire-kind checks still run.

Core exchange integration library. See also exchange-specific `CLAUDE.md` files in `src/exchanges/*/`.

> **CLAUDE.md is a living spec.** When you change non-obvious
> behavior here or in any per-exchange CLAUDE.md, update the relevant
> doc in the same commit as the code change. See the root
> `CLAUDE.md` → "RULE — CLAUDE.md files MUST be kept current" for
> the directive.

For exchange migrations that add first-class opening trigger/conditional orders,
read `CONDITIONAL_ORDERS.md` before changing mapper or order lifecycle code.

## DELEGATED handover

`sealDelegatedHandover` is an additive, synchronous, host-neutral sender for
DELEGATED handover for a hosted-supported venue. Its standalone module needs no WASM, Node Buffer or
browser base64 globals; the host supplies the usual secure random source for
Noble. Call it only after the host's hosted-signing and server-origin eligibility check. It does
not select signing modes, open sockets or change LOCAL/COSIGNED setup.
Inputs contain the host's padded-base64 X25519 public key and attempt/boot/venue
context; the result contains only the attempt ID and padded-base64 encapsulation
and ciphertext. Context fields preserve exact UTF-8 bytes and cannot contain
the protocol's `|` separator. Optional passphrases preserve absence versus empty.
The key-only registry mapping (currently Paradex) supplies `signingKey` in
sealed credentials and `secret: ''`; the public input `key` and init key equal `subkeyAddress`, never
the private scalar. The optional private field is not copied into the result or
HPKE context. The receiver rejects scalar-in-init layouts, foreign credential
layouts and empty CEX secrets; existing trusted setup validates actual material.
HMAC callers omit `signingKey` and retain byte-identical envelopes.
The sender uses RFC 9180 base mode with X25519/HKDF-SHA256/ChaCha20-Poly1305 and
one fresh encapsulation per call. Deterministic RFC and shared synthetic handover
fixtures exercise the same production sender. The shared fixture lives with
the server tests for receiver interoperability; Rust consumption depends on
the server handover implementation landing.

## DELEGATED order outbox

`OutboxOptions`, every connection ticket, persisted intent and proof bind the
authenticated `owner`, account label, actual `venue` and immutable `accountId`.
Hosts must change `accountId` when account credentials, network or admission
identity are replaced, and preserve it through socket reconnects. Storage keys
and sequence checkpoints include this entire scope. A replacement mismatch
retires the old ticket and refuses before handoff. Restore rejects an unbound
or mismatched scope before any writes/compaction; it preserves the stored
history for host reconciliation and never adopts it into the current account.
Restore arms expiry only after all rows and final compaction succeed. Every
failed open retires the partial instance and cancels its timers, including if
scheduling itself fails; already queued callbacks cannot write after retirement.

`enqueue(params, { intentId })` accepts a caller-owned canonical lowercase UUIDv7.
Invalid IDs throw `DelegatedIntentError` with code `invalidIntentId`; a different
canonical payload for the same ID throws `intentConflict`. Identical retries
return the existing durable ID without another sequence, deadline or submission,
including after restore. A failed original put refuses retries with
`intentNotDurable`. Caller-key reservation updates only the last successfully
committed snapshot, preserving its handoff state and evidence even if a newer
receipt could not be persisted. A retry never makes that newer outcome durable,
renews replay permission, or replaces a committed handoff with an in-memory
queued label. Persisted no-handoff claims still pass the evidence constructors.
Caller-key rows remain reserved through ordinary history
compaction, within the existing total capacity of 1,056 records; exhaustion
refuses new intents rather than forgetting a key. Hosts must preserve scoped
storage and close a writer before replacing it.

Saved accounts and exchange options accept optional `signingMode` using the
existing account-worker wire values `local`, `cosigned`, and `delegated`.
`accountToExchangeOptions` forwards the account mode and permits a validated
override. Absence preserves existing signer selection; this metadata does not
activate DELEGATED writes or replace admission. Unknown modes (including null)
are refused as `invalidInput` / `notSent` during conversion and factory creation.

Sam's 2026-11-11 policy allows activation with place and reduce-only close before
the other venue actions exist. `activate(connection, { DELEGATED })` checks
the host-owned feature flag, the captured box/router transport proofs, and only
the advertised `delegatedActivationScopeV1` place/close capabilities. Discover
box capabilities using `?exchangeName=<joined venue>` and retain its
`delegatedVenueV1`; activation refuses a missing or mismatched venue. The neutral
`DELEGATED` flag replaces `DELEGATED_BYBIT`, and `delegatedDisabled` replaces
`delegatedBybitDisabled`. `requireDelegatedActivation` takes the connection venue
as its third argument. `OutboxConnection.venue` is an `ExchangeName`; its trusted
low-level connection still requires a remote-signing-supported venue. Amend, venue
cancel, cancel-all and nuke are not activation prerequisites. A failed activation
retires the old ticket, including deferred work, and throws a named reason.
The existing low-level `connect` API still requires prior host admission.
`enqueueAction(actionName, params)` permits place or a close with literal
`reduceOnly: true` and visibly refuses every other string action before storage
or a frame; it never chooses another signing mode. Supported string actions
capture JSON parameters once before validation; fingerprints, persistence and
handoff use those captured values. A close requires literal `reduceOnly: true`
in the captured payload. Router queued-intent cancellation keeps its separate
existing semantics. These admission helpers do not alter handoff, deadline,
restore, evidence or replay rules below.

`DelegatedOutbox` is an opt-in host-neutral module for admitted DELEGATED single-order
submits. Both the box's advertised `delegatedOrderFramesV1` and the router's
`delegatedOrderLedgerV1` must be explicitly true in the captured connection ticket;
missing/false flags disable writes and receipts. Hosts inject scoped persistence,
secure randomness, a monotonic clock with a stable epoch, deadline scheduling
and an admitted socket connection. Close the outbox on logout; pass the captured connection ticket to flush and receive.
Router outcomes may be renumbered per client socket, including stored replays.
Receipt ordering is checked per intent within that ticket, so one intent's higher
number cannot hide another's deferred replay. Replacement tickets reset receipt
ordering and old tickets cannot update the replacement connection.
The router contract is pinned at NX #9404 `9f52873b76`. Unrecorded cancellations,
coverage uncertainty and ledger failures answer unknown, never prove not sent.
Receipt kinds are decoded as literal strings; hostile JSON values cannot invoke
string coercion. Capability discovery still requires completed socket admission.
Intent UUIDv7 keys become Bybit client order IDs unchanged. Physical handoff marks
an intent `sending` in memory; sending, acknowledged and unknown intents are never
submitted again. At ten monotonic seconds from creation (including persistence
and admission waits), a never-forwarded queued intent becomes `notSent`; an
in-flight or acknowledged intent becomes `unknown`. Direct close also changes
those retained entries to unknown immediately while fencing later work and callbacks.
Disconnect/replacement makes
in-flight and acknowledged intents unknown immediately. The synchronous send
adapter returns true only after socket handoff; only an explicit false guarantees
neither handoff nor buffering for a later connection. A throw or any other non-true
return supplies no such evidence: it becomes unknown immediately, loses its replay
proof, and is never automatically submitted again, including after reconnect or restore.
Queue cancellation after handoff remains unknown until a router
receipt positively establishes non-forwarding. Legacy pending cancellations restore
as unknown. Cancellation or expiry during a declined first send remains terminal
and is published; a declined retry cannot erase a previous handoff.
`notSent` is a branded state, constructed only by the exported
`markDelegatedNotSent(intent, evidence)`. It requires positive evidence for the
whole intent: a locally known never-attempted intent, a declined adapter handoff
with no earlier possible handoff, or an admitted authoritative router refusal.
A failed reconnect attempt cannot prove an earlier handoff did not occur.
The private runtime brand is read only by the outbox; plain state writes cannot
manufacture the result. JSON stores the evidence kind and its owner/account/venue/incarnation,
intent ID, sequence and fingerprint binding. Restore validates that evidence and
reissues the runtime brand; legacy or mismatched `notSent` claims become unknown,
including their unsupported error outcome. No-evidence clock-epoch/deadline
expiry of a restored queued record is also unknown rather than proof of no send.
Restored ambiguous handoffs are unknown. Once enqueue persistence succeeds, temporary unavailability while
preparing handoff retains that queued intent and its ID for retry. The NX router
supplies durable deduplication and queue cancellation;
see `docs/plans/callers/delegated-order-frames.md`. This module does not select a
signing mode, open sockets, add an HTTP write path or change LOCAL/COSIGNED calls.

The outbox bounds pending work to 1,024 entries and keeps the latest 32 completed
records. The in-memory completed index refers to the same objects in the live
intent map and credits only durably completed records. Retirement removes both
entries through one private helper. Capacity accounting and compaction prune
stale index entries before use, so a stale entry cannot release capacity or
participate in sorting. Unknown work, including pending queue cancellation, is retained for 24
hours from its UUIDv7 Unix creation time, using the injected Unix clock across
reloads (clock rollback can extend retention). At expiry it remains `unknown`,
gains `retentionExpired: true`, stops pending queue cancellation, and emits the
normal `changed` notification. Hosts must surface this as expired, unresolved
history and reconcile via ordinary orders/history; expiry proves nothing about
the venue outcome. Only after that explicit expiry state is durable may it join
the latest-32 completed history and retire. Expiry never enables resubmission or
changes an unknown to `notSent`; a late positive router refusal can still prove
non-forwarding. Storage failure keeps the row for a later flush retry.
Storage adapters implement atomic `compact(scope, checkpoint)`: delete only the
listed retired intent IDs in that scope and advance its durable sequence using
the maximum of old/new values. Preserve all other records. `load` returns an
`OutboxSnapshot` with sequence progress and records (record arrays require the
same complete account binding). `put` preserves checkpoint metadata. Compaction failure
keeps records until a later flush retries. Unrelated retirement failure never
rejects enqueue or hides its durable intent ID after handoff. Close the old
outbox before replacing it with a new writer for the same scope.

## DELEGATED account actions

`DelegatedOutbox.enqueueAction(action, options)` retains the separate typed
action API, gated by the boolean `delegatedOrderActionsV1`, independently of
`delegatedActivationScopeV1`. It adds the versioned `order.action` family for
amend, venue cancel, cancel-all and nuke. Existing `enqueue` placement frames and
`cancelQueued` semantics stay unchanged. Action writes/receipts require both
`delegatedOrderActionsV1` (box) and `delegatedOrderActionLedgerV1` (router), as well
as the existing two placement/ledger flags and completed DELEGATED admission.
The action-specific fingerprint includes its version, kind, target IDs/symbols
and update values in a distinct canonical domain. UUIDv7 intent IDs use the same
router dedupe namespace across both families; never reuse one for another action.
Both entry points accept optional `{intentId}` with a canonical caller UUIDv7.
Identical retries are durable identity lookups: preserve sequence, canonical
payload/fingerprint, deadline and handoff evidence, and never submit again.
Different action/targets or placement family rejects `intentConflict`; a failed
original put rejects an identity retry as `intentNotDurable`. A later state-put
failure never erases the original identity's durability or authorizes replay.
The additive
`callerSuppliedIntentId: true` reservation survives completed-history retirement
and restore under the same 1,056-record bound. `enqueueAction` shares placement's
not-sent and replay evidence constructors; no direct not-sent assignment is
permitted outside `markDelegatedNotSent`. Deadline expiry leaves a handed-off
action unknown; queue cancellation is explicit, never a venue cancel.

Action outcomes are typed `applied`, proven `refused/notSent`, `unsupported`
for a pre-dispatch schema rejection, or `unknown`.
Applied results survive later uncertain queue-cancel replies. A proven refusal
also survives later uncertainty, while its subsequent authoritative refused
outcome may supply the refusal message after a negative ACK.
A replayed positive receipt ACK cannot revoke that refusal or clear its bound
evidence, including after disconnect, close and restore.
Partial, interrupted and unproven results stay unknown and never become
resendable. Nuke uses the existing box account operation: stop algos, cancel
orders and close positions, awaiting all legs. Its queued work carries the
original registration and rechecks it after load/store waits; retirement drops
stale replies. Action persistence uses version-2 rows with the exact action and
result, retaining atomic scoped checkpoints/retirement and all version-1 placement
records. See `docs/plans/callers/delegated-action-frames.md` and the shared
`delegated-actions-v1.json` fixture.

The entire typed DELEGATED WS API is one version, v1. New operations extend v1
additively, without per-operation versions; v2 is reserved for a major redesign.
Unknown operations or fields must fail closed with a typed `unsupported` outcome,
never silent omission or execution of the known subset. Unsupported outcomes
remain terminal across restore without creating replay evidence. Storage row
versions are independent of the wire version.

The additive full-write vocabulary uses the same v1 action frames. Batch and
protective placements, reduce-only close, settings, symbol/batch/account cancellation,
close-only/stop-only nukes and native algo/EXEC operations additionally require
`delegatedFullWriteActionsV1` and `delegatedFullWriteLedgerV1`, together with the
four existing flags on the admitted ticket. Old four-action routers confer no
expanded admission. Native dispatch/completion stays on the hosted account;
there is no browser-sign or legacy-method fallback. Algo action receipts remain
unknown and reconcile from account events, never an assumed applied result.
Typed TWAP-state rows preserve optional `ownerGeneration` as a nonnegative safe
integer alongside `ownerId`; it is included in the fingerprint, never synthesized.
Native restoration still requires both tags to match its trusted binding.
Accepted long-lived jobs keep their account scope; queued admission and EXEC
continuations keep their captured registration/deadline fences. The complete
closed payload/outcome contract is in `docs/plans/callers/delegated-action-frames.md`.
LOCAL and COSIGNED method forwarding is unchanged.

Algo parameter amendment is deferred: `chaser:amend` and `twap:amend` remain
unsupported, including before durable SDK enqueue. Existing TWAP pause/resume
keeps the same job and schedule; never emulate amendment with automatic
stop-and-replace.

The hosted typed v1 action decoder rejects duplicate object keys recursively in
placement orders, close orders, scheduling values and algo payloads before they
can hide unknown or malformed fields. Binding/hash checks retain precedence;
schema rejection returns typed `unsupported` before ACK or native work.

Native account-wide cancellation requires the venue's `nukeAccountCancel`
capability; it never falls back to an incomplete symbol census.

## Mirror CI invariants (don't "clean these up")

Bitget TP/SL metadata is account/path-specific through `getExchangeMetadata`:
`bitgetAccountMode: 'classic' | 'uta'` and `tpslOrderPath: 'entry' | 'position'`.
Classic/unknown entries offer no selector while their preset default is
UNPROVEN; supported UTA entries and position TP/SL offer Last and Mark. Inverse
UTA has no native route and offers no selections. The conservative
static list is empty for Bitget and both siblings. Web consumers must adopt the
path-aware call; see `docs/plans/callers/bitget-tpsl-trigger-types.md`.

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
KuCoin Futures/Spot also install their passphrase and optional configured broker
key only in this capsule. One receipt binds Present plus the request, passphrase,
and optional broker MACs. Both names admit RemoteSigner through device-owned
`kucoin-batch` custody, including the secondary read slot; PayloadSigner remains
refused because a request MAC cannot supply the separate passphrase/broker MACs.
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
explicit empty never inherits. Local signer setup in public mode bypasses only
the primary requirement, while secondary validation stays active. Shared publicMode intent takes precedence
over legacy publicDataOnly, including null/omitted requested. This declaration does
not enable signing for an unmigrated family.

Native public-mode parity uses trusted signer setup's normalized credential
presence, including per-family Rust whitespace rules. Below SDK admission,
Coinbase/Spot and TxFlow private payload intent without a native provider refuses with
`invalidInput/notSent`; an installed provider reaches `unsupported/notSent`.
Retained WASM fixtures must distinguish these cases rather than equating callback
intent with provider availability.
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
AsterDEX V3 native requests use closed `asterdex-wallet` statements instead;
the device derives account-bound EIP-191/712 digests and the engine verifies the
returned signature. `asterdex-address` proves wallets for nonce setup, and
`asterdex-hmac` selects V1 primary/secondary custody. PayloadSigner is V1 REST only.
They retain real qs/Axios serialization and caller-owned WS preimages. Capture
tokens are single use and fenced by raw deadlines, protocol expiry and clock
revision; provider text and partial results are not returned on failure.
A native `dispatchSigning` checkpoint alone does not enable these hosted APIs;
its family must also pass exact legacy-wire and actual WASM/SDK acceptance gates.
The BingX native checkpoint keeps hosted admission disabled until those gates.
WOO local and RemoteSigner accounts share provider-owned dispatch signing for all
V1/V3 private REST calls and private WS auth. V1 signs sorted form/query bytes plus
`|timestamp`; V3 signs timestamp, method, path/query and the exact body. Replayed
serialized bodies are preserved. WOO has no signing nonce or wire recvWindow.
PayloadSigner remains refused because its legacy private lifecycle has no such
mode; RemoteSigner cannot fall back to it or to local material. Before-dispatch
signing failures are `notSent`; dispatched ambiguous writes remain `Unknown`.
Private WOO RemoteSigner setup refuses any configured secondary credentials with
`unsupported` / `notSent` before native construction or callbacks: its HMAC wire
cannot select the device's read-key slot. Remove the secondary credentials to
use Hosted with the primary key, or use read keys in local browser mode.
ApeX Omni admits RemoteSigner through `apexomni-batch`: device-held OmniKey
contracts precede base64-text-key HMAC signing. Origins retain their API
passphrase in Present-only provider custody; the secret and OmniKey stay on the
device. Private PayloadSigner and secondary read keys refuse before I/O.
ApeX HTTP POST captures sign sorted semantic form fields, decoding serialized
form/multipart bodies while preserving transport bytes and existing zkLink signatures.
See `docs/plans/callers/apexomni-hosted-signing.md` for the wire and parity contract.

Crypto.com admits RemoteSigner and canonical HMAC PayloadSigner. Every signed
REST envelope and `public/auth` uses provider custody; the hosted wire carries
only `method + id + api_key + canonical params + nonce`. PayloadSigner keeps
private REST reconciliation and writes but opens no private user socket.
Crypto.com HTTP callback metadata follows its declared header policy in native
dispatch and hosted capture; the HMAC remains in JSON `sig`. WS auth refuses
callback result headers.
Crypto.com Hosted secondary credentials refuse with `unsupported` / `notSent`;
all native private POSTs retain primary selection. Subaccounts use their own
API key/secret, with no separate wallet or cosigning protocol.
Phemex admits RemoteSigner (hex HMAC, no PayloadSigner); its hosted REST codec
in `rust/legacySigning.ts` signs `path ‖ query ‖ expiry ‖ body` as the reference did.
Trusted setup removes both local credential slots from generic options; its
REST and user.auth recipes use provider custody and the signed expiry-only receipt.
Effective public mode in delegated setup is resolved before custody checks,
including keyless accounts with unused callbacks. It preserves raw mode-intent
flags but ignores signing callbacks, credentials and setup metadata. It installs
no provider or identity, advertises no algorithms, and cannot sign. Private-mode
callback validation, primary/secondary selection and mode refusals remain strict.
ApeX uses this shared early return for saved-key and keyless accounts explicitly
requesting public mode. Captured public mode cannot be changed by later
construction input; no separate ApeX public-mode guard is installed.

SDK factories and the Rust constructor reject OUT venues before reading options.
For admitted venues, construction snapshots outer signer fields and their methods
together under one guard before mode resolution. Admission, asynchronous factories and signer setup
share that immutable snapshot, preserving receivers without further caller reads.
Its opaque construction token is stripped before generic options are retained.
Throwing getters or Proxy traps refuse with a fixed, redacted `invalidInput` /
`notSent` error before transport; caller errors are never retained.

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

Backpack's `backpack-ed25519` device codec resolves `[Present, Sign Ed25519]`
atomically for the selected primary/secondary slot. Registry engines hold only
provider refs; private startup and WS use them without raw credentials. Private
PayloadSigner remains refused. The direct native raw-options compatibility API
retains its previous WS path. The caller document above owns parity and hold
requirements; betaVenues.ts keeps its existing refusal wording.

The beta registry always selects Rust in SDK factories and `ExchangeWorker`;
`ExchangeOptions.engine` remains an accepted compatibility input only.
`src/__tests__/cutover/*.wasm.test.ts` verifies the worker frame/store consumer
contract for every `RUST_VENUES` name; actual native startup and independent
binary lifecycle fixtures have separate evidence scopes. See phase 9 of
`docs/plans/RUST_REWRITE.md` for deletion blockers; no TS venue may be removed
merely because its name is registered in Rust.

## Exchange event listeners

`utils/diagnosticRedaction.ts` makes detached diagnostic copies before console,
log events, telemetry or retained diagnostic buffers receive them. It shares the
HTTP diagnostic string policy; live command and market messages stay untouched.
Its parity fixtures are pinned to the host diagnostic contract at `6fde9450cf`.
Structured `ws-diagnostic` events use this same detached redaction boundary before
emitter listeners and worker forwarding; live-result events retain their identity.

`BaseExchange` uses a local TinyEmitter subclass for both TS and Rust venues.
One throwing listener cannot starve later listeners for the same event; errors
are reported through `console.error` with venue, event and original cause.
Reporting emits no error/log event, so failures in those listeners cannot recurse.
The inherited once/off/context and subscription-snapshot behavior stays intact.

Rust `startup.incomplete` carries bounded per-section cause fields from the native
engine. `BaseExchange.emitAndLogError` derives Sentry tags from those event fields
for local and hosted accounts; it never guesses causes from browser read state.
See `docs/plans/callers/startup-incomplete-causes.md` for the field contract.

Managed HTTP failures also emit the JS host's Debug `http.request_failed` event,
with the actual SDK venue plus method, redacted path template, phase and closed
failure kind. Queries, origins and request material are never diagnostic fields;
this event does not emit another error or change retries/native deadlines.

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
Shared ticker `observedAt` carries source engine receipt times for last/mark/index,
including unchanged observations. SCXF's optional TICKER_OBSERVATIONS section and
an opt-in shared-feed handshake preserve those times across frame/port/in-process
hops; polling, snapshots and local REST restoration never date them as now.
Older producers retain changed-value-only dating; older native consumers receive
only the legacy JSON shape. Only genuinely observed fields gain freshness.
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
market batch and propagates partial or unknown outcomes. On a venue with
`freshPositions`, flat comes only from a `readPositions` body
`{"skipped":[symbols],"open":[{"symbol","side","contracts"}]}`, the store rows the
read committed; the read runs even with no rows in the store, and at least
`NUKE_READ_SETTLE_MS` after a close round. A nonempty `open` closes again, within the
rounds, then reports did not leave flat. A failed, timed-out, malformed or old array
read sends no close and reports not confirmed flat, as do skipped symbols.
`nuke` and `nukeOrders` first cancel every traded symbol (known orders, open
positions, attempted placements on either engine), or the venue-wide cancel on a
`native` venue; a failed cancel is reported and does not block the close. Where a
symbol cancel misses orders the store has not seen (`symbolCancelMissesUnseen`), a
venue with `nukeAccountCancel` also gets the account-wide `cancelAccountOrders`,
which nothing but a nuke sends; any other such venue's nuke result carries a warning
(`NukeResult.warning`, and the end of a failure message) to check open orders.
`nuke` also attempts the venue's cancel-all operation after a close failure;
the original close failure remains the result. Concurrent nukes are rejected.
Every nuke (`nuke`, `nukePositions`, `nukeOrders`) first stops the account's
running TWAPs, then its chasers, waiting for each chase's cancel (the worker's
in its Rust algos via `algoStopAll`; an in-process CLI's TS algos via
`webworker/stopAccountAlgos.ts`, matched by exchange identity); otherwise a
live chase re-places behind the nuke. It resolves with
`{ stoppedChasers, stoppedTwaps }`. The hosted server's exec nuke does the same
and ends with the venue's cancel-all sweep.

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

Optional SDK helpers needed before engine startup are declared under
`bridge.helpers` in `construction-rules.json`. The native generated construction
record and synchronous SDK record share that declaration. Install and dispatch
those helpers from their declared flags, never from a venue-name check.

Each Rust venue reports a capability record (`safe_cex_engine::Capabilities`,
read through `EngineHost.capabilities()`): account identity, client-order
lookup, history axes, fills by order id, native cancel-all, leverage reads and
maximum, account-wide leverage and margin type, symbol-routed cancel/amend,
order-book queries and options. Clock selection is shared engine-host policy:
the default SDK transport awaits shared `virtualClock` startup, anchors an
epoch-shaped scheduling clock to `performance.now()`, and never moves its live
deadlines on subsequent synchronization. Browser workers wait up to 30 s for
an initial host time push before anchoring. Later syncs update only the engine's
protocol UTC fallback; small corrections preserve venue calibration, while a
>1 s host-baseline discontinuity (including suspend) invalidates it. The default
transport records wall-versus-monotonic divergence as pending UTC data;
EngineHost drains it outside HTTP handoffs before the next Rust call. A detected
step also defers a shared server-time resync to refresh the cached wall-clock
offset. This permits a one-off request in browser workers; ordinary worker sync
still uses page pushes. Resync notifications run after the current handoff. Journal
UTC bounds and scheduled-cancel validation still use synchronized wall time.
Explicit host/recording transports retain their own clock; native servers supply
their OS clock and own its synchronization. See `docs/plans/callers/engine-monotonic-deadlines.md`. There is no per-venue
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

**Frame-fed main engine.** A keyed mirror whose family the wasm module reports
`frameFedSupported` builds its own engine with `frameFed: true`, the account's
key and route, and starts it only when `construction().frameFed` is true (no
venue streams, no account reads). `frameFedReady` resolves whether it did;
otherwise the mirror path below continues for that account. The host feeds it
the data source's SCXF frames with `ingestSourceFrame(bytes, snapshot)` (a
delta before the first snapshot is dropped, so it asks the source for one) and
the engine's own frames fill the UI store. Books, candles and fills still come
from the source's ops. It signs and sends `FRAME_FED_WRITES` (manual writes,
settings, nuke, scheduleCancel) itself, including writes the web sends through
`callMethodInWorker`; a nuke first asks the source for `stopAccountAlgos`. A
write whose outcome is unknown fences its symbol. From a source advertising
`orderReadCovers`, a fence lifts only on a read the source sent after the write
(`ORDERS_READ`, docs/plans/RUST_FRAME_FED_READ_COVER.md). The engine emits
`orders.held { symbols, liftAfterMs }` (comma-joined, `''` the account) whenever
a symbol not yet named turns uncertain or a partial lift leaves some standing,
and the page asks the source for `refreshAccountOrders(symbols)` once
`liftAfterMs + 500 ms` has passed: one timer, the latest names, the earliest due
time. The source's engine (`refreshOrders`) bounds the reads. A plain Unknown
cancel has no placement fence: for a source-refreshing venue,
`orders.refresh { symbols }` asks the source once without disturbing that
placement-hold timer. It preserves the cancel's Unknown error, never replays,
and never infers a Canceled order from absence.
Bitget also sends bounded `orders.cancel_check` targets before that same ask.
The source registers them through read-only `registerCancelChecks`, then runs
`refreshOrders` even if registration refuses. Only its clean post-registration
native census can report present/absent before cached-algo retention; it never
changes the writer's original Unknown command result.
A LOCAL worker forwards one account's frames, stamped `{op: 'frame', data,
snapshot}`, after the `forwardFrames` op, and keeps posting fill ops.

The web builds a second `RustEngineExchange` on the main thread; the worker's
instance owns the engine. An instance with a `callMethod` listener (the host's
worker bridge) forwards the `WORKER_FORWARDED_METHODS` in `exchanges/base.ts`
(trading, nuke, client-ID lookup, public trades, candles, history axes; plus
the native account reads `fetchOrders`, `fetchPositions`, `fetchBalance` and
`fetchPositionMode` by name, since those wait on native state and would
otherwise start a local engine; the worker answers them as
`WORKER_OPTIONAL_METHODS`, and a mirror's `syncAccountState` is a no-op because
worker frames keep its store current) whole through `callMethodInWorker`, so
preflight, placement scopes and nuke retries run against the worker's state and
report there once. `fetchHistoricalPositions` is NOT forwarded: the mirror
loads the worker's capability record and folds the journal itself with the TS
mappers, from forwarded raw reads (fills, orders, income, and the raw
`fetchClosedPositions` rows), because the hosted Rust server has no TS mappers
and serves only those reads. The hosted server answers `fetchOrders` and
`fetchPositions` with `{ accountFrame }`, its engine's base64 snapshot frame,
which the mirror applies to a scratch store with `FrameApplier`, so hosted rows
carry the same native-unit extras and NaN handling as a worker's. This holds even if a
main-thread read (`fetchMarkets` during warm start) started a local engine; the
worker never has that listener. Without a bridge, an instance runs locally.
`updateOrder` crosses as `{ orderIds, clientOrderId }` because array properties
do not survive a JSON bridge. Interrupted forwarded writes carry
`outcome: 'unknown'` and the caller's client order IDs (`WORKER_WRITE_METHODS`,
`workerCallClientOrderIds`); requests still buffered when a worker stops before
startup are answered `notSent`. Cancel-all and nuke wait five command deadlines.
Owner fencing is optional and additive. Without `setWorkerOwner`, calls retain
legacy bridge-wait and dispatch behavior, including calls queued before attachment.
With the hook installed, forwarding captures the bridge subscription
identity and the host owner binding at admission, before page-engine startup,
and checks both again before dispatch. A removed or replaced bridge
drops the pending call; an undispatched write rejects as `notSent`. Reattaching
the same callback is a new subscription, even when the account name is reused.
Hosts opting into fencing install
`setWorkerOwner(() => ({ ownerId, generation }))`, reading the current login's
authenticated principal (never an account label) and a monotonic, nonnegative
safe-integer generation. Change the generation on retirement/replacement;
return `undefined` after logout. An installed hook without a current owner fails
closed; attaching the first listener cannot establish its earlier owner. An
unchanged bound owner can bootstrap normally. Changing the provider itself also
retires pending work.
The direct source-order refresh and capability-adopted `fetchClosedPnl` use the
same optional admission fence. Only the existing public candle read is permitted
to retry across a replacement bridge.

A `workerMirror` instance keeps the account's own options, credentials
included, because the web starts the worker from
`{ ...exchange.options, workerMirror: undefined }`; its signing host and any
engine it builds read the stripped `engineOptions`. Stripping the mirror's
options left the worker keyless: a public-only engine with no private event.

Every public mirror method is classified in
`__tests__/audit-regressions/rust.mirror-forwarding.test.ts` as forwarded to the
worker, mirror-local, unsupported, or a write (moving to the main-thread writer
engine); an unclassified method fails typecheck. Engine reads forward
(`WORKER_FORWARDED_METHODS`, plus the Rust-only `fetchStpMode` and
`fetchOrderBook`). `fetchLeverageForSymbol` and `fetchSymbolMaxLeverage` answer
from frame-fed rails and ask the worker only for a venue read. On a mirror,
`validateAccount`, `fetchEngineSnapshot` and `snapshotFrame` throw a named
`unsupported` error, and `fetchFillsForOrderIds` is never adopted;
`WorkerMirrorExchange` is the type without them. Forwarding decides a method is
an inert fallback by its brand, not by the capability record a mirror learns
later.

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

HyroTrader validates its configured mode/network URLs before loading WASM and
reports a missing configuration through the normal error event as
`invalidInput/notSent`. No venue or simulator address is inferred.

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
Partial information with empty order IDs still exposes the attempted total and failed
index when supplied, plus `partialOrderIds: []` and `partialOrderCount: 0`.
That zero counts created IDs; applied cancellation effects remain in `elements`.
Recorded `elements` retain each logical row's index, optional client order ID,
placed IDs, applied effects with existing-target references, refusal or unknown
detail through host errors and worker/CLI clones. Applied effects create no IDs;
accepted partial IDs without row attribution remain unknown.
Malformed arrays are discarded whole; nested errors copy only declared venue fields.
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

## Native reported closed-PnL output

`fetchClosedPnl` returns generated `ClosedPnlEntry` rows directly from Rust
`history_output`. All seven financial fields and both timestamps are required
nullable values: unknown inputs and unknown/overflowed derived results stay null;
measured zero stays zero. Missing close time never borrows opening time, and
unknown close times sort last without dropping the row. Numeric strings remain
accepted by the public contract for derived/reference producers. Consumers must
propagate unknown through totals rather than coerce null to zero.
Bybit inverse sizes, OKX contract conversion, fee signs,
account labels and newest-close ordering are native output policy. The four TS
closed-PnL mappers live only under test references. `fetchClosedPositions`
remains the raw journal contract; it is never a fallback for reported PnL.
The capability-installed method forwards whole to the worker when bridged,
including after capability adoption; it never commands a mirror-local engine.
The Bitunix/Blofin income helpers and Bybit inverse helpers also live only in
`__tests__/reference`; their remaining callers are replay oracles and tests.
The production venue-file inventory removes them with the runtime imports.

## Reported closed-PnL read failures

Rust `fetchClosedPnl` propagates the engine's typed read error, including through
worker/hosted transport. A failed read is never a successful empty history;
refreshing consumers retain their last good result and surface the failure.
Actual empty reads and public-only sessions still return empty arrays. Journal
and ingestion use the separate strict `fetchHistoricalPositions` path, so this
change confers no ingestion completion or new history capability.

## Native history row diagnostics

Bitget and Kraken row refusals carry `causeCode: HISTORY_DECODE` and a closed
`historyDecode: { schema, reason }` record through host and worker errors. The
record contains no row values, identities or response text. Only Bitget and
Kraken enable the non-default `history-row-diagnostics` engine feature; the JS
validator remains available when native detail is absent. Bitget rejects a
proven invalid row immediately instead of replaying the same page; transport
and envelope failures retain their bounded retry policy. Native strict checks
and Kraken's existing accepted-prefix incomplete policy remain unchanged.

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
private) open/first-frame/close state, at one property write per carried channel
per frame. `SocketSpec::public_topics(true)` declares that a primary private
socket also carries public topics; it preserves routing/authentication and counts
the one physical socket in both health channels. Hyperliquid and TxFlow use it
only when their public topics are direct; separate sockets remain independent.
`RustEngineExchange` exposes one `RustSocketHandle` per channel through the
`BaseWebSocket` fields, so `getAllSockets`, `reconnect`, `recoverAfterOffline`,
CLI `ConnectionMonitor` and the base `dispose` work unchanged. It writes
`store.socketHealth` through the shared `summarizeSocketHealth`, on socket
changes and every 5 s, never through the TS watchdog. A private socket is healthy
for 60 s of silence, the engine venues' own idle limit. Reconnecting closes only
open sockets; a connect in flight is left alone, as in TS. `Deactivate` and
`auth.rejected` events raise `authFailureStreak`; a private stream delivering
again resets it. Private `privateLastMessage` retains the last received frame's
page-clock timestamp while the socket is uninitialized or reconnecting; a feed
that has never received a frame still has no timestamp. Health/initialization
flags remain independent of this diagnostic age.

`store.latency` is nullable: null means no measurement, including on reset.
STATUS keeps its fixed layout and uses `LATENCY_MEASURED` to distinguish an
actual zero sample; positive legacy samples remain readable. Frame-fed engines
and worker stores preserve null. UI callers must hide unknown latency.

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
  **Rejecting venues**: AsterDEX, HyroTrader, Kraken (+spot),
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

## Protective edits never drop the protection

Most venues amend a stop or take profit by cancel-then-place, so a replacement
the venue refuses would leave the position unprotected. Before the cancel, the
engine (`sdk-rs/engine/src/protective_amend.rs`) refuses `notSent` an amend
whose new trigger already fires against the order's own reference (mark or last
per its trigger type, both when unknown) or sits over 10x from the symbol's
price (a price meant for another symbol), but only against a reference observed
in the last 5 s: each of mark, last and index carries its own observation time
(`Store::reference_seen_at`), set when it changes or when a venue reports it
unchanged (`observe_ticker_fields`; a delta omission is not an observation).
Attached and virtual legs (edited in place on the parent) are exempt. A venue
that judges the trigger itself (`Venue::judges_protective_amend`: Bybit's
Partial drag, Hyperliquid's TP-to-SL only when mark, last and index agree within
25%, BingX's native kind) keeps only the 10x guard. When Binance or Bybit
definitively refuse a replacement after its cancel landed, the old order is
placed again exactly (a Bybit Limit stop keeps its limit price), once; the error
then carries `protection: 'restored' | 'unprotected'` (`ExchangeError.protection`,
kept by `serializeCommandError`). An unknown outcome is never restored, and a
stop the cancel found already gone (filled or triggered) is never put back.

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
entry — `bybitinverse.exchange.ts`, `bitgetinverse` and `okx` all
compute `notionalUSD / (mark || last)` — so that product equals
`notional * entry / mark` and drifts with the market. It is only the notional
at `mark == entry`.

On top of that, **bybitinverse and bitgetinverse leave `unrealizedPnl`
base-denominated** (okx converts; these two do not), so on those
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
use it. The helper returns `number | null`: coherent native evidence wins, absent
native uses derived size, and conflicting/unavailable quantities remain unknown.
No per-field freshness signal exists; close/reduce consumers refuse a conflict
rather than selecting either value by magnitude. CrossTrade validates canonical
quantities in both reconciliation passes. Unknown master/copier rows remain
present and refuse automatic sizing; known-flat rows remain distinct. Matched
reductions compare exposures projected from canonical native quantities, and
lot-rounded reductions are capped at the known copier quantity. The missing-
position pass also refuses unknown opposite-side rows, since an entry could
reduce them. Market, LimitSlippage and Chase forward these validated amounts.

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

Rust public capabilities also retain the loaded engine record, including
`orderHistoryNeedsSymbol` and other authoritative policy flags. Those fields
remain absent before engine/worker capability adoption; method-derived booleans
retain their existing checks. Symbol-scoped collectors must read this record,
not guess from a venue name or send a symbol-less query as an empty census.

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

## `payloadSigner`

`ExchangeOptions.payloadSigner` signs elsewhere without giving the SDK a local
secret. It receives the exact venue payload and returns the signature plus any
permitted result headers. SDK factories select Rust, including Binance and Bybit;
there is no TypeScript venue fallback. The shared delegated-provider companion
consumes callbacks only for aliases enabled by `construction-rules.json`, keeps
their declared REST-only private-feed policy, and strips callbacks and signing
credentials from retained exchange options.

Public-mode resolution happens before unused custody checks, using each family's
normalized credential presence and explicit mode intent. Private callback setup
requires a configured provider; missing configuration refuses with
`invalidInput` / `notSent`. Unsupported private custody refuses with
`unsupported` / `notSent` before transport. Below SDK admission, native
Coinbase/Spot and TxFlow payload intent without a provider gets the former;
an installed provider reaches the latter. Native intent flags alone are not a
callback provider and do not enable an unmigrated family. See the shared
construction and delegated-provider contracts above and
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
Paradex declares both `supportsTunnel` and `enforceWsTunnel` true because local
account REST already selects HOW, including unsigned market discovery. This
describes the existing adapter contract, not a live-connectivity guarantee.
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

Managed HTTP carries the engine's closed `requestClass` into HOW outside signed
packet bytes. TS ignores non-write hints on non-GET methods; explicit remaining
engine deadlines still govern managed posted reads. Catalogue discovery alone
gets 60 s (90 s during startup): markets, instruments, contracts and symbol/asset/
product catalogues, including closed catalogue RPC kinds. Live public depth,
order books, tickers/market summaries, trades, candle polls and funding/mark-price
reads get 20 s on every family; unknown public read kinds also stay short. Engine
history queries (fills/order history/income) get 30 s, while candles and public
trades use 20 s. Semantic classification happens once at the shared managed-HTTP
boundary and uses the original Context issuance time, so neither re-prepare nor
retry renews the deadline. Explicit nonstandard venue bounds and shorter Plan/
caller caps remain authoritative. Both engine and transport diagnostics name
class, budget and elapsed milliseconds; readiness stays inside the original
bound, with no write replay.

Managed engine HTTP uses `Transport.prepareHttp` to finish route/proxy-token
and tunnel readiness before signing. `credentialIntent` (`public`, `primary`,
`secondary`) selects the route independently of HTTP method or credential headers;
custom routing providers must honor it. A custom HTTP/tunnel adapter without the
prepared interface fails explicitly instead of invoking its legacy sender.
Preparation and binding never send HTTP. Binding preserves signed URL/query/body
bytes and rejects conflicting host headers; `ready()` rechecks the original
request deadline and resource, and `dispatch()` invokes fetch/WebSocket.send
synchronously after the engine release claim. A bound resource with `admitDispatch`
first checks shared admission asynchronously, then invokes claim plus dispatch
synchronously within its handoff callback. Grouped sends refuse these individual
guards before claim. Direct callers of guarded dispatch perform the same check.
Cancellation releases only that
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

**Fleet REST fallback allowlist.** `transport/tealstreet/proxyFleetAllowlist.ts` must
match the active HTTPS ACLs in `terraform/private-exchange-proxy/haproxy-sg-running.cfg`.
Its test also requires each advertised route to select an existing backend; commented ACLs
are not routes. Remove retired DNS names from both files together, or
`engine.ts` can select an HTTP fallback the fleet cannot serve. WEEX remains
HTTP-only (no HOW); adding its fleet route does not enable the venue or change
that transport policy. Lighter's `testnet-api.lighter.xyz` was documented as retired
and previously excluded. The SDK inventory now includes every fixed REST base
declared by the venue crates, including that testnet URL; the source-derived
coverage test includes production, demo/testnet and auxiliary APIs. Configured
HyroTrader endpoints are caller-owned rather than fixed venue bases. SDK coverage
does not prove endpoint reachability: HAProxy config and the fleet rollout remain
a human step, including resolving the retired Lighter endpoint before routing it.

**`howEgress` is the one derived field — it is NOT a literal in the table.** The HOW (REST-over-WS) tunnel egress region (jp/sg) is owned by the `@tealstreet/http-over-websocket` package, which is what actually picks the egress at request time. The routing table computes `howEgress` via `isTokyoEgress(name)` imported from `@tealstreet/http-over-websocket/src/egressRegion` (the single Tokyo-egress list), so the table never duplicates that list. The exception is the region-pinned family (bybit when tunnelling), whose api factories force the HOW adapter region from `proxyRegion`; those are declared `regionConditional`. The single-source lock — table's name-based egress must equal the adapter's runtime baseURL-based egress for every exchange — is enforced in `routing.equivalence.test.ts`. To move an exchange's HOW egress, edit the list in `egressRegion.ts`, not here.

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

**Nothing credentialed goes to the public proxy (`transport/publicProxyGuard.ts`).**
Engine `prepare` refuses, before I/O, any request bound for `publicCorsProxy()`
with primary/secondary intent, an `Authorization`/venue key header or a
`signature`/`sign` query param (`SignedPublicProxyRefusal`, code
`HTTP_PREPARATION_INVALID`). Secondary read keys are deprecated, so their default
public-proxy reads fail closed, and the native plan omits a public-proxy
`readHttp`. Lighter's `usesRestProxy` profiles (`payloadSignedViaPublicProxy`)
keep payload-signed REST there. The native short-lived token
(`deadline:account:key-index:hex(80-byte signature)`) may appear in `Authorization`
or the `auth` query parameter only on that deployment's exact API origin and this
explicit method/path allowlist:
GET `/api/v1/trades`, `/api/v1/accountInactiveOrders`, `/api/v1/accountActiveOrders`,
`/api/v1/positionFunding`, `/api/v1/accountLimits`; POST `/api/v1/changeAccountTier` (the existing user setting).
Every other venue credential header and every `signature`/`sign` query parameter
is refused, including on Lighter. Binding repeats the check on the signer's final
URL, query tokens and venue headers before raw binding; refusal releases the
preparation. Legacy binding checks the captured send URL and refuses any URL change.
Proxy-owned authorization is separate from the venue's signed headers.

**Secondary read keys are bypassed (`src/secondaryReadKeys.ts`,
`SECONDARY_READ_KEYS_ENABLED = false`).** `withSecondaryReadKeyPolicy` drops the
saved read key before Rust signer setup and in the engine transport/plan, so
signed reads use the primary key and route, with no warning. The account's
`secondaryProxyUrl` remains only as the egress for managed unsigned public GETs
(TS `publicReadHttp` and the native plan's `publicReadHttp`, no `readHttp`),
except direct-public venues; POSTs and header-inferred GETs keep the account route. Coinbase drops that egress as
before. Every secondary path below, and the NX UI and flags, stays intact behind
the switch; tests pinning today's behaviour mock it to `true`.

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
grvt, apexomni, lighter, polymarket, and the orderly ed25519 family). A read-only "second API key" is
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
│       │            AsterDEX, BitUnix, BybitDemo
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
  missing"), naming the last `/api/proxykey` failure when there is one
  (`lastProxyKeyFailure`: HTTP status, timeout, network error or no token). The edge's plain-text 401 (`Authorization header is missing` /
  `Invalid JWT`) reaches a Rust engine as a transport failure, after a token
  refresh, never as a 401: venues classify a 401 as a refused key
  (`auth.rejected`). The engine transport reports either failure once per
  outage through `createEngineTransport`'s `onProxyAuthorizationFailure`, which
  `RustEngineExchange` sends to `emitAndLogError`. A venue's own 401 passes through.
- **Static validation**: `exchangeMetadata.test.ts` has invariant tests that catch impossible configurations (e.g., `requiresCors + forbidCors`). Run `yarn test-ci` after changing metadata.
- **Unknown metadata fails closed**: `getExchangeMetadata` throws for runtime
  names outside the exhaustive registry. Never restore a venue-shaped fallback;
  it can apply the wrong transport and capability flags to corrupt saved data.
  Retired names stay saved on accounts (`bitmex`, `bitmextestnet`), so every
  lookup refuses `unsupported`/`notSent` and the caller fails that one account:
  factories and the worker throw `unsupportedVenueError` (`ExchangeError`), and
  `getExchangeMetadata` throws an `Error` carrying the same fields, because the
  Rust source oracles (`sdk-rs/safe-cex-types/scripts/accounts-source.cjs`) load
  this file without the WASM host. Do not import `@tealstreet/safe-cex-wasm` here.
  The WASM `venueFamily` and Rust `registry::unsupported_venue` match it
  (`__tests__/audit-regressions/removed-venue-refusal.test.ts`).
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

Rust engine sockets also emit the opt-in `ws-diagnostic` event with a
`WsDiagnosticEvent` object: `{ venue, channel, direction, message }`. Channels
are public/private/business and directions are send/receive. This is generic
across venues, including Hyro. The package exports `setWsDiagnosticsEnabled`,
`isWsDiagnosticsEnabled` and the event type. The switch defaults off in each
realm; worker hosts use the existing `__workerConfig.wsDiagnosticsEnabled` flag.
`ExchangeWorker` forwards `{ op: 'ws-diagnostic', data: [event] }`; consuming web
controllers must handle that op and re-emit it before rendering Requests Log rows.

Diagnostics run after socket I/O and validate both field names and values.
Only closed protocol/status/error enums, bounded integers and success booleans
survive. IDs, topics, scalar data, messages and other free-form strings become
fixed placeholders, including nested/encoded echoes inside retained fields.
Arrays retain at most 32 sanitized entries. Challenge/signature material,
connection URLs/headers and binary contents are never included. Opaque,
malformed, oversized and excessive-depth content is withheld. Heartbeats are
omitted, output is capped and frames are sampled per socket. Disabling the
switch drops queued entries too, even if it is re-enabled before their callbacks
run. Queued socket diagnostics capture the cancellation generation and recheck
it before each structured/legacy callback. Existing private grafana events remain
available with the same sanitized frame text; do not display both as duplicate rows.

## Worker Position Refresh Bridge

`BaseExchange.updatePositionsInWebworker()` emits the `updatePositions`
worker op handled by `src/webworker/handlers/UpdatePositionsHandler.ts`.
Account-wide refreshes (`symbols` omitted) must use
`store.addOrUpdatePositionsFull(positions)`, including empty arrays, so
positions omitted by the exchange are removed. Last reported per-ID leverage/margin/mode survives separately in the generated `positionSettings` map. Symbol-scoped refreshes
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

KodiakFi, WOOFi, Mode, and What use one Rust venue
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


Chart consumers must honor `exchangeMetadata.supportsKlineSeconds` and optional
`supportedCandleIntervals`. Hyperliquid and HyperliquidSpot share their native
interval set, which excludes seconds and 6h; neither may reach a candle subscription.

### Candle history across account-worker replacement

`fetchOHLCVInWorker` is authoritative for Rust mirrors as well as CandleCache.
Only Bitrue, Lighter and LighterRobinhood opt in. Other mirrors lazily own a
credential-free public source using the same endpoint/routing options, disposed
with the mirror. Frame-fed account engines execute only writes; candle reads
never run on them. The real-WASM candle-routing test pins this boundary.
Candle reads do not wait for private reconciliation or lift an order fence.

A detached bridge fails pending reads with `WORKER_STOPPED`. Reads name the
read interruption; uncertain writes retain their existing code, reconciliation
warning and outcome. Worker candle reads alone retry once through the replacement bridge,
using its existing 30-second attachment bound. No write is replayed. Tests in
`rust.candle-routing.test.ts` and `rust.worker-command-contract.test.ts` pin
page routing, replacement replies, the bound and zero write replay.

Relay-failure notices strip URL queries from upstream error messages before
reporting reconnect state. Request configs and signed dispatch bytes are retained
unchanged; the web Sentry hooks provide independent diagnostic redaction.

Unsigned Bybit mainnet market responses with HTTP 429 or native `retCode`
10006, 429 or legacy 10018 generate
one causal transport diagnostic through the exchange log/Sentry channel (no
additional toast). Only bounded egress IDs and numeric X-Bapi-Limit-* /
Retry-After fields are copied; URLs, bodies and arbitrary headers are excluded.
Old tunnels without node metadata report unknown. This observer changes no
response, routing or retry behavior; the Rust venue's local kline pause remains.
Observation runs at the shared host-transport response boundary for managed and
legacy requests on tunnel, proxy, direct and public-CORS routes. Streamed bodies
are observed lazily with a 1 MiB bound for native-code detection; incomplete or
failed streams cannot supply a native-code notice. HTTP 429 reports once at the
response headers regardless of body content, size or stream completion, without
consuming the body. Status-only notices carry HTTP 429 instead of a guessed
native code.


Hyperliquid `/info` admission shares a token bucket across account workers through
IndexedDB. Prepare may wait but reserves no weight. Admission durably reserves
weight before the final shared Retry-After/generation gate and physical handoff.
Cancellation or Claim without dispatch refunds only that token generation; a
request handed to fetch is never refunded. Once IDB has worked in a realm,
storage failure waits within the original read deadline rather than sending from
an unshared memory budget. Never-opened IDB keeps the memory-only behavior.
Role/mode startup reads
stay single-flight until their managed completion or deadline, including quota wait.
Each Hyperliquid engine issues at most three routine account reads at once, with
core Mode/State/Orders ahead of ancillary work. Queued intents acquire their
managed deadline only when issued. Foreground Orders, including refresh asks,
Unknown reconciliation and owed successors, retain priority but consume the same
slots; promoting a queued intent preserves its captured census epoch. Caller-owned
history, positions and settings retain their independent admission and original
quota/root deadlines. Only completion of a slotted read releases its slot. Shared
dispatch waiters use FIFO leases so newer cheap reads cannot starve older Orders;
100 ms renewal and 2 s expiry bound a dead worker's obstruction without charging
its abandoned work.

Explicit native prerequisites and fence reconciliation precede account financial
reconciliation, then normal ancillary reads and bulk caller history/candles in both
admission layers. Account State/Orders, reconnect snapshots and Role/Mode/Spot
use the reconciliation class; its 1,000-weight ceiling shares the critical reserve,
while explicit fence/amend reads retain first rank. The three-slot financial bound
stays in place. Startup fees/builder are queued until financial reads settle; a live
placement can release its queued builder check with its ordinary class and bound. The 1,000-weight
bucket refills at 1,000/min; noncritical work can occupy 800, reserving 200 for
critical reads. Native refill is continuous. Pending native metadata supplies the
closed class through the Hyperliquid-only WASM `httpReadClass` export; a caller
orderStatus lookup stays bulk. Classes never enter HTTP bytes or grant authority.
Weight-blocked preparations keep their leased FIFO position through dispatch.
Candles retain their original 20 s native bound, capped by a shorter caller wait;
queued expiry cannot turn into a late send. See `docs/plans/callers/hl-critical-read-priority.md`.

The worker's unchanged 120 s timeout preserves closed native startup fields and
per-account `/info` route, 429, quota-wait, undispatched and in-flight counters.
`startup_native_sample_age_ms` identifies the age of the last native watchdog
snapshot; elapsed/deadline tags on the timeout describe the worker's own bound.
No request bodies, account addresses, headers or URLs enter these facts.

Public Hyperliquid catalogue reads (`perpDexs`, `metaAndAssetCtxs` and
`spotMetaAndAssetCtxs`) share byte-exact replies across engines through a separate
IndexedDB cache. Entries expire after 5 s; a 3 s leader lease bounds follower
waiting and fences late leaders. Cached preparations recheck payload expiry at
readiness and handoff; asynchronous publication owns a separate byte buffer.
Cache storage attempts are bounded to 250 ms;
unavailable storage bypasses the cache while retaining ordinary quota admission.
Only anonymous public requests with the exact catalogue shape qualify. Account
reads, candles and writes are never cached. Typed discovery failures invalidate
that origin's entries and leases. Cache hits make no physical request and consume
no quota. Invalidation after native parsing uses the closed `discovery` failure
kind supplied by #3046; HTTP/JSON errors already invalidate at the cache boundary.

The same `SharedCatalogue` coordinator owns anonymous Binance `exchangeInfo`
GET sharing. Binance and Hyperliquid have separate closed eligibility/validation
policies; account requests, writes, tokens and candles never participate. Binance
keys include exact endpoint/query, network and resolved public route. Valid replies
have a 5 s TTL and an 8 MiB cap; leases last at most 25 s and never exceed their
owner's original managed deadline. Followers never bypass an active lease or
cooldown. Failed physical reads publish owner-fenced 1/2/4/8/16/30 s cooldowns,
honoring Retry-After up to five minutes before followers can elect again.
Unavailable storage from startup preserves independent native discovery/backoff.
After a successful coordination transaction, storage loss refuses preparation
instead of bypassing an unknown lease. Waiter cancellation never deletes a peer's
lease/cooldown; each engine still parses and publishes its own catalogue.
The 30 s storage recovery pause starts only on an actual failed storage attempt.
Retries refused during that pause cannot extend it; after expiry the coordinator
reopens storage and rechecks the existing route-scoped lease before sending.
Future-dated strict ownership stays blocking. A dispatched timeout publishes a
cooldown before abort cleanup without delaying the caller; expired ownership
retains its minimum failure window until that publication can settle. The original
deadline callback remains authoritative after wall-clock rollback. Queued Binance
publications check the transport lifetime at the storage write, including after
a timeout has already completed request cleanup.

Rust account `socketHealth.reconnectDiagnostics` carries bounded last-close
snapshots for public/private/business channels and the primary HOW adapter,
plus maximum hidden timer lateness across engine/HOW callbacks. These are TS
host observations, not native venue health or server-managed browser evidence.
Snapshots retain object identity until a bucket changes; the existing health
sync publishes them without a new polling loop. The wasm binary is unchanged.

`updateSymbolMetadata` rechecks exact symbol membership after native market readiness
before querying leverage. A foreign symbol or an engine retired during readiness
starts no metadata command. UI callers also validate their account market binding.

Bybit native receive-window corrections emit bybit.clock.corrected with numeric
offsetMs only. Hosts without an explicit clock-advice listener keep the normal
info log; CLI listeners replace it with one clock-sync warning per session.

## Position type ownership (P1)

`src/generated/Position.ts` and its enum/Greeks dependencies are generated from
`sdk-rs/safe-cex-types`; `src/types.ts` imports/re-exports them for the existing
public API. Run `corepack yarn types:generate`, then
`corepack yarn generate:script-types`; never edit generated declarations or the
script bundle. The Position-only compiler gate is
`node --test sdk-rs/safe-cex-types/tests/position-contract.cjs`.

Position notional is required `number | null`. Unknown native valuation survives
JSON as null; numeric consumers must propagate unknown or refuse, including
notional-based totals, ratios and sizing. Quantity remains independent and drives
position presence. Shared helpers return null for incomplete valuations.

The native engine owns Position display normalization and native sizing extras.
Positions use SCXF section revision 2 with eleven numeric columns and required
ID/symbol references; the host refuses older Position sections. Deploy matching
native/WASM and SDK artifacts together. Other section revisions are unchanged.
Live position snapshots are authoritative; omitted rows are removed rather than
retained as fabricated empty positions. NX must land its corresponding consumer
and artifact changes before this cutover. See `docs/plans/TYPE_OWNERSHIP_P1.md`.

Position enum-word bits 24 and 25 declare the presence of native contracts and
native PnL columns. A present non-finite native size is rejected, never treated
as absent (which could select an unsafe base-unit fallback).

## Generated Store contracts (P1)

StoreData, StoreOptions, StoreDataLoaded, StoreUpdateIds, FetchedLeverage and
DisplayedMargin come from `sdk-rs/safe-cex-types/src/store_model.rs`. The scoped
generator emits their modules and the initial Store value; never hand-edit them.
Other models and host-owned capabilities retain their owning modules. Regenerate
with `corepack yarn types:generate`; the scoped compiler gate is
`node --test sdk-rs/safe-cex-types/tests/store-contract.cjs`.
Opaque legacy config members and userData are unknown and require narrowing.
