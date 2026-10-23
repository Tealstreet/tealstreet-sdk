# Remote signing wire v2 and provider credential import

Plan, 2026-10-23, base `8ea5850dba9`. Owner: claude-19pt8s; overseer
claude-ddi2il. Two shared signer seams from
[RUST_SIGNER_NON_HMAC_MIGRATIONS.md](RUST_SIGNER_NON_HMAC_MIGRATIONS.md):

- **P8:** extend the remote-signing wire with additive, versioned formats for
  non-HMAC families. Existing HMAC formats stay byte-identical.
- **P6b:** import Polymarket's L1-derived API key into provider-held refs.

Sam decided both on 2026-10-23. This document is a plan; it changes no code.

## P8: versioned sign operations on the existing channel

### Today

- **Wire:** one `sign-req` / `sign-res` envelope. `RemoteSignerRequest` is
  `{id, type: 'http' | 'ws-auth', exchangeName, accountName?, config?, wsAuth?}`
  (`packages/safe-cex/src/types.ts:1294`), mirrored in tealstreet-next. There
  is no version field and no capability handshake (`InitPayload`,
  exchange-server `server.ts:329`).
- **Server:** exchange-server forwards requests opaquely
  (`remote-signer.ts:578-669`). Its failure policy: a 15 s timeout, and 10
  consecutive errors spanning at least 30 s force-dispose the account.
- **Rust bridge:** `remoteSignerProvider.ts:262` maps only `HmacSha256` work
  onto `ws-auth` and refuses every other algorithm, `result` parts and the
  secondary slot. Keyless non-HMAC accounts are refused at
  `rust.exchange.ts:826`.
- **Browser:** `ExchangeServerWorker.ts:879` signs `http` in the worker and
  sends everything else to `exchangeAccountController.handleSigningRequest`
  (`:3020`), which assumes `ws-auth`. An unknown type reaches `signWsAuth` and
  returns an error. That error feeds the fatal streak, so it must never be
  triggered by design.

### Design

1. **Request shape.** A new request type keeps `t` as the first JSON key, so
   the browser's `{"t":"sign-req"` fast path still sees it:
   `{t:'sign-req', d:{id, type:'op', v:2, exchangeName, accountName, op | ops}}`.
   - `op` = `{recipe, slot: 'primary', inputs, receipt}`.
     - `recipe` is a closed per-family recipe id, such as
       `hyperliquid.order` or `coinbase.restJwt`.
     - `inputs` are that recipe's structured canonical inputs: the EIP-712
       domain, type and message, the action and nonce, or the JWT claims and
       instruction fields. They are never a bare digest or opaque bytes.
     - The recipe, not the server, fixes the algorithm and the key role, so a
       `zklinkSeed` or `l1Wallet` key can never be applied to another recipe.
   - **Credential interpolation.** The server interpolates only the
     `publicCredential` of the same identity as the signing key, which it
     already holds from init, as `remoteSignerProvider` enforces today. A
     credential part naming a secret or extra ref is refused. Keyless
     `Sha256` steps run server-side, and `Present` ops use init values.
   - A `result` part means a multi-operation batch, which the server sends as
     an ordered `ops` array; each later operation names its `result` index
     and encoding.
   - `accountName` is required for `op`, and the browser refuses to broadcast
     an `op` to all accounts.
   - It never reuses `http` or `ws-auth`. An old browser would HMAC a reused
     type with `secret` and return a wrong signature.
2. **Response shape.**
   `sign-res d: {id, v: 2, values: base64[]}` or `{id, v: 2, error}`.
   Widths are validated per algorithm (`SignRequest::validate_result`). A v2
   request answered without `v: 2` is an error.
3. **Capability negotiation.** The browser advertises
   `signOps: {v: 2, recipes: [...]}` in `InitPayload`, which is
   included in `getInitFingerprint`.
   - `recipes` lists the recipe ids from the venue features compiled into
     that browser's WASM build. It is not a list of algorithms: a browser can
     support `es256` without the Coinbase recipe.
   - The server sends an `op` only for a recipe that session advertised.
     Otherwise the Rust provider refuses before sending (`unsupported` /
     `notSent`). That refusal never reaches the wire, so it cannot feed the
     fatal streak.
   - An old server forwards `signOps` into its `getInitFingerprint`
     (stringified), so a change rebuilds the account. It does not act on it.
     An old browser never advertises, so it never receives v2.
   - Capability is captured per account at construction, never latched per
     session.
   - If init lacks any recipe the family requires, keyless construction is
     refused, extending today's `rust.exchange.ts:826` refusal. The account
     never runs half-broken with per-request refusals.
   - Mobile WebView, CLI and custom-script sandbox clients never advertise,
     so they are safe by construction.
4. **Browser executor.** The answer comes from a local provider built from
   the same trusted setup as the account's dispatch provider, never from TS
   venue classes.
   - `RustEngineExchange.signOperation(op)` → WASM `hostedSigning({kind: 'op', …})`.
   - That extends `registry/src/hosted_signing.rs` `Input` with
     `Op {recipe, inputs}`, dispatched per family in the registry. This
     replaces today's hard-coded binance/bybit gate; `safe-cex-venues`
     already links every venue crate.
   - Each venue exposes one canonicalizer, preimage to digest or message plus
     classification, used by both the server recipe and the browser, so the
     two cannot drift.
   - It reuses `LocalSigner::sign_now`, including role binding of extra refs.
   - **Deadlines.** Raise the browser cap from the current 5 s, keeping:
     browser cap + round trip < the server engine's sign deadline < the 15 s
     `SIGN_TIMEOUT_MS`. An abandoned wait must never reach the server-side
     fatal timeout.
   - The effective remote signing deadline is never longer than
     `min(original root bound, family freshness, transport/provider cap)`.
     A late reply after a shorter root bound expires is discarded, with a
     control test for it.
5. **Algorithms.** `ed25519`, `es256`, `secp256k1Digest`, `starkDigest`,
   `zkLinkContract` (after #1839/#1844 land), and Lighter Schnorr after P3.
   - `Sha256SecretSuffix` (Bitunix) and `HmacSha512` (Kraken) have no remote
     mapping yet, so keyless Rust Bitunix and Kraken stay unsupported.
   - Each family's adoption enables only its own recipes in
     `construction-rules.json` `remoteSigning`.
   - Coinbase's existing `coinbase-jwt` path stays for TS-hosted accounts
     until its Rust family adopts `es256` ops.
6. **Failure semantics.**
   - A provider refusal or timeout before release is `notSent`.
   - A late or duplicate `sign-res` is fenced by id and attempt, as with
     `ws-auth`.
   - No HMAC format, `extraData` or payloadSigner scheme changes.

### Security: no blind wallet signing

The browser today sees each HTTP request and its venue interceptor's rules,
including Hyperliquid's refusal of fund-moving actions. HMAC API keys also
carry venue permission scopes. Wallet and L2 keys do not:
- a Hyperliquid main wallet can sign withdrawals and transfers;
- Polymarket's L1 wallet holds USDC (permits and approvals);
- Stark and zkLink keys can sign L2 transfers.

A compromised server must not be able to obtain arbitrary signatures. No
algorithm is opaque: every op names a family recipe, the browser recomputes
the signed bytes with the shared canonicalizer, and it applies that family's
content allowlist before signing.
- **Wallet and L2 keys** (secp256k1, Stark, zkLink, Lighter Schnorr) sign
  only orders, cancels, leverage, margin and auth logins. Polymarket
  ClobAuth is not an auth login for this purpose (see below). They refuse
  withdraw, transfer, approveAgent, permit, approval, vault moves and L2
  transfers.
- **Polymarket ClobAuth is refused on the remote path.** A ClobAuth signature
  is all the server needs to derive or create the L2 key itself, which would
  defeat the hosted-custody rule. It stays refused until a reviewed
  device-side derive exists.
- **Hyperliquid** refuses remote ops when the address derived from the
  signing key equals the account's main wallet address. Only a distinct
  agent key signs remotely; Hyperliquid agents cannot withdraw or transfer.
  An address mismatch is necessary; the content allowlist still applies.
- **Exact-message families** are content-checked too:
  - Backpack: an instruction allowlist (orders, cancels, account reads);
    `withdraw` and similar instructions are refused.
  - Coinbase: decoded JWT claims. The `uri` method and path face the same
    deny rules as the HTTP interceptor (no transfers or sends), `aud`/`iss`
    are fixed, and `exp - nbf` is at most 120 s with `nbf` near the device
    clock.
  - Orderly: the method and path are checked against the venue. Its
    withdrawals need a separate wallet EIP-712 signature, so the risk is low.
- Refusals are typed and never sign.
- **Sequencing.** P8 for a family depends on that family's dispatch
  migration first. Hyperliquid and Polymarket still sign with raw private
  keys in-venue and have no provider recipe yet.

### tealstreet-next changes (for its owners; not made here)

| Area | Change |
| --- | --- |
| `packages/safe-cex/src/types.ts` (copy) | `type: 'op'`, `v`, `op`/`ops`, `values` fields; consume the SDK release that adds them. |
| `services/exchange-server/src/server.ts` | Accept `signOps` in `InitPayload` (already in the stringified `getInitFingerprint`). Pass it to Rust account construction as that account's capability. |
| `services/exchange-server/src/remote-signer.ts` | No protocol change. Confirm v2 errors are counted like any `error`, and that refusals before sending never reach the wire. |
| `apps/web/.../ExchangeServerWorker.ts` | Advertise `signOps` from the loaded SDK. Route `type: 'op'` to the signing exchange (`signingHost`), not to `handleSigningRequest`. Refuse an `op` without `accountName`. |
| `signingHost.ts` | Register accounts whose signing material lives in `key`, `auth` or `omniKeySeed` too. Answer `op` through `exchange.signOperation`. |
| `exchangeAccountController.ts` | Add a third arm for `type: 'op'` on the main-thread path. |
| Custom-script sandbox (`PostMessageRemoteSigner.ts`) | Unchanged. It may opt in later. |

**Confirmed today (NX lane, read-only, 2026-10-23).**
- Hosted Paradex accounts already send the Stark subkey or ETH private key to
  exchange-server: `options.key` is in `INIT_FIELDS` (`secret` is dropped,
  `key` is not), and the server maps it to `apiKey`.
- Polymarket would send its wallet private key the same way. Polymarket is
  disabled, so there is no production exposure.
- claude-ddi2il has raised this with Sam; this plan makes no NX change.

Consequences for this plan:
- Before P8 admits keyless hosted signing for either family, NX must stop
  sending their signing material, for example by moving it out of `key` or
  excluding it from `INIT_FIELDS`.
- **SDK guard (conditional; not implemented).** This waits on Sam's pending
  decision: stop now, or keep until the hosted-leases hardening. If adopted,
  hosted (`hostedMode` / `remoteSigner`) Rust construction for Paradex and
  Polymarket is refused when `key` parses as private-key material. The engine would otherwise hold a key it is meant never to see.
  The refusal is typed (`invalidInput`, at construction) and never echoes the key.

### Proof

Both halves need tests for each adopting family:
- **Server half:** a keyless Rust account with an injected fake
  `remoteSigner` that records `op` requests. Assert the exact recipe inputs,
  and refusal when the recipe is not advertised.
- **Browser half:** WASM `signOperation` with a local setup. Assert exact
  vectors.
- **Content allowlist refusals:**
  - withdraw, transfer, approveAgent, permit and approval are refused;
  - ClobAuth is refused remotely;
  - a Hyperliquid main-wallet key is refused;
  - Coinbase claim bounds and denied `uri` paths are refused;
  - a Backpack withdraw instruction is refused;
  - an Orderly method or path outside the venue is refused;
  - Stark and zkLink L2 transfers and vault moves are refused;
  - an unadvertised recipe is refused before sending.
- **Shared canonicalizer:** the server recipe and the browser produce
  identical signed bytes for the same inputs.

Cross-half tests replay recorded requests. Compatibility tests cover an old
browser with a new server (never sent), and a new browser with an old server
(no change). No live keys are used.

## P6b: provider credential import (Polymarket L2)

Agreed split (codex-hcmq3f, claude-55ao7t, 2026-10-23):
- **Session core (#1832/#1842):** owns *when* `/time` + derive run. That is a
  session key mint under a session owner, re-derived via `retire_session` and
  `open_session`, with a ReadFailure retry inside its bound.
- **This seam:** owns *installation* only.
- **The venue:** sees only status and non-secret metadata.

1. **Setup.**
   - The wallet key is `ExtraKey::L1Wallet` on the primary identity (declared
     hex, required for private).
   - A non-secret provider derivation (P4) returns its address. The venue
     never derives it.
   - L1 `ClobAuth` EIP-712 is a recipe: `Secp256k1Digest` over the venue
     digest, with the `l1Wallet` role.
2. **Import request.**
   - `Cx::request_import(context, url, step, ImportSpec)` stages the derive GET
     like any signed managed request.
   - `ImportSpec` names the JSON fields to install and their roles:
     `apiKey → credential`, `secret → signing` (base64url, declared),
     `passphrase → passphrase`. It also names the non-secret fields to echo.
3. **Response routing.**
   - The managed entry and generation-liveness check runs first. A response
     for a retired generation is zeroized and never imported.
   - Otherwise, on a 2xx response the engine passes the body bytes straight
     to `Signer::import(ImportRequest {slot, generation, spec, body})`.
   - For import attempts only, `on_import` replaces `on_response`; transport
     loss still goes to `on_request_failed`. The session plan
     (RUST_BACKGROUND_SESSION.md, "no new callback") must be amended to say
     so; this is coordinated with claude-55ao7t.
   - Import responses skip unproven-HTTP evidence, reconciliation and
     streaming. `request_streamed` is forbidden for imports.
   - The import body buffer is zeroized after handoff and redacted from
     Debug, request logs and verbose response streaming. The WASM path hands
     over bytes, never a JS string, because JS strings cannot be wiped.
   - The provider parses, validates, decodes and installs refs bound to
     `(slot, session generation)`, and returns the ref names plus echoed
     non-secret fields. The engine then calls the venue's
     `on_import(pending, Result<ImportReceipt, VenueError>)`.
   - A non-2xx or unparseable body is a typed failure: status and the venue
     code are kept, and secret-shaped fields are never echoed.
4. **Retirement hook.**
   - `Signer::drop_imported(slot, generation)` is called by
     `retire_session` (claude-55ao7t) and on engine disposal. It is
     idempotent and fire-and-forget, and it runs again on re-open.
   - Provider-issued ref names embed the generation. The engine checks it at
     sign time, as does the provider.
   - Recipes that reference a retired ref fail with `Unavailable`, and the
     session core re-derives.
5. **Provider surface.**
   - The local provider implements `import`.
   - Injected JS and Workers providers get an optional `import` callback; if
     it is absent, the import refuses `unsupported`.
   - **Hosted custody.** In hosted mode the server engine performs all HTTP,
     so a server-executed derive would put the L2 secret in the server's
     response body. That is forbidden.
     - P8 signing ops alone do not provide a remote import wire.
     - Keyless/hosted Polymarket therefore stays unsupported, even after P8,
       until a separately reviewed path exists for device-side import and
       ownership of the derive HTTP request. Only then would the derived key
       live on the device.
   - Injected JS and Workers providers implement `import` and receive the
     drop as `Action::DropImported` (section 7), together or not at all.
6. **Credential conditions (claude-m26ps4, 2026-10-23).**
   - **Slot.** Import installs refs only for the slot whose L1 identity
     derived them (primary today). The derived refs form a session identity
     bound to (slot, generation) beside the L1 identity, never overwriting
     it. Venues reach it only by selecting that slot, with no new selector.
     Secondary import is refused.
   - **Subject.** Echoed non-secret fields (address, funder, API-key id)
     never become the account subject or a `stable_id` by themselves. A
     subject is used only after it is validated equal to the L1-derived
     signer or funder.
   - **Nonce.** The ClobAuth nonce `0` and timestamp stay protocol data.
     Imported refs never key a nonce domain.
   - **Generation.** A selection made before retirement never signs with
     dropped refs. A stale-generation recipe refuses `Unavailable` before
     sending (`notSent`).
7. **Session-core conditions (claude-55ao7t, 2026-10-23).**
   - **Binding.** Imported refs bind to the selected slot and the opaque
     engine `Owner::Session { generation }`, not a venue counter.
     `request_import` accepts only a live session-owned Read context.
     Command and Background contexts are refused `Invalid`. The derive GET is
     a deterministic session Read, so it needs no write facts.
   - **Retirement order.** `retire_session` drains waiters, then discards
     in-flight HTTP, then revokes, and only then drops imported refs. The
     engine checks generation liveness immediately before routing a body to
     `Signer::import`, so a late 2xx for a retired generation is never
     imported.
   - **Host providers.** Host-injected JS and Workers providers receive
     `Action::DropImported { generation }` through the existing provider
     action seam. The local provider drops synchronously. Engine disposal
     retires every live session, which drops all imports.
   - **Readiness.** On `on_import` success the venue calls `session_ready`,
     releasing the #1842 waiters. On failure it calls `session_failed`
     (`NotSent` fan-out), then applies its backoff.
   - **Credentials.** `SessionPolicy.credential` stays the L1 wallet ref.
     Contexts that sign with imported refs name them in their own Credential.
     The engine rejects a context naming refs from a dead generation before
     the provider, as pre-send `Unavailable`.
8. **Ownership.**
   - Engine changes (`Cx::request_import`, response routing, the
     `Signer::import` and `drop_imported` trait methods) need uufub4 sign-off
     for signing/dispatch and m26ps4 sign-off for credential semantics.
   - claude-55ao7t calls the retirement hook from the session core.
   - codex-hcmq3f wires the Polymarket continuation.

**Import fencing (codex-hcmq3f, 2026-10-23):**
- Before routing a body, the engine validates the exact current session
  Context and attempt, not just the generation.
- Installation is atomic: either every declared ref is installed or none is,
  and a partial install refuses and wipes.
- Async, late and duplicate import receipts are fenced by attempt and
  generation, so retirement can never reinstall stale refs.
- Controls: a late response after retirement, partial-install failure, and a
  duplicate receipt.

**Out of scope:** when no API key exists yet, Polymarket needs a create-api-key
POST. That is a Mutation with different retry rules. Today's venue has no
create fallback, and this plan adds none.

**Proof:**
- the venue never observes body, secret or passphrase bytes (a canary
  assertion on every venue callback);
- refs bound to the generation are dropped on retirement;
- a stale-generation recipe refuses;
- malformed, oversized and duplicate-field bodies fail closed;
- import is refused when the provider lacks it;
- the L2 HMAC through the imported refs matches the upstream base64url vector.

## Sequencing

1. This plan is reviewed by claude-ddi2il, uufub4, m26ps4 and 55ao7t, and
   NX owners are pinged for the P8 table.
2. P8 SDK/Rust half (`hosted_signing` Op, bridge `type: 'op'`, capability
   gate), after #1839 and #1844 land.
3. P6b engine/provider seam, after #1832 and #1842 land.
4. Family adoptions consume the actual pins.

Architecture gate: both are shared signer, provider, host and session choke
points. No family code holds keys or imported secrets; a venue contributes
canonicalization and an import field declaration only.

## P6b implementation plan (Sam decision 9, 2026-10-23)

Owner claude-19pt8s. One PR, stacked on #1839 (`ExtraKey`), #1844
(secp256k1 arm) and the reviewed session-core heads #1832 and #1842. It is
retargeted as they land.

**Split of work.** This PR delivers:
- the shared primitives (L1 role, address metadata, the import seam,
  provider/host import and drop);
- the Polymarket recipes in `venues/polymarket/src/sign.rs`;
- proof through a test venue on the session harness.

codex-hcmq3f's Polymarket managed conversion is a separate, later PR. It
moves the venue onto session contexts, wires the recipes and import, removes
every in-venue key and secret use (listed in phase 7), and carries the
real-venue positive proof (trading tests 161 and 441 through refs). This
mirrors the P8 per-family obligation. Sam's "no in-venue derivation" is met
when that conversion lands, and this PR's body says so. **Polymarket keeps no L1 private key, no L2 API
secret and no passphrase in venue state, and derives nothing from them.**

**Phases.** Each phase is a commit with focused tests.

1. **L1 primitives** (claude-m26ps4 credential semantics; codex-uufub4
   schema).
   - `ExtraKey::L1Wallet`: a 32-byte secp256k1 key, hex (an optional `0x` is
     stripped by trusted setup), required for private use. It is role-bound
     to `Secp256k1Digest` and `PublicAddress` only. Setup rejects a zero or
     out-of-range scalar.
   - Setup returns non-secret public metadata (the 20-byte address)
     alongside `Identities`. The venue caches and checksums it, so startup
     needs no extra round trip.
   - `Operation::PublicAddress { key }` (fixed 20-byte result) is used only
     when a host provider supplies no setup metadata.
   - The local provider implements `Secp256k1Digest` and `PublicAddress` for
     this role. Recovery 0/1 maps to v 27/28 in recipe `apply`, and the
     vectors assert it.
2. **Identity shape.**
   - Polymarket's static primary `Identity` holds `extra[l1Wallet]`.
     `credential`/`signing`/`passphrase` are empty unless L2 values were
     supplied directly.
   - The setup policy declares the slot "complete via extra" (field
     `requiredForPrivate` on `l1Wallet`), so `apply_to_identities` validates
     it without an L2 credential.
   - Imported refs never mutate the static `Identity`. On `on_import` the
     receipt carries a per-generation session `Identity`
     (credential/signing/passphrase refs plus the same `l1Wallet` extra).
     L2 `SignStep`s use that `Identity`. The engine checks its membership for
     the (issuer, identity, slot, generation, attempt) tuple.
   - The primary slot binds to `CredentialRole::SessionAuth` through the
     session policy credential: the static L1 identity's slot.
3. **Supplied-L2 path.** Today `api_key`, `api_secret` and `api_passphrase`
   can be configured directly. They become primary-identity trusted setup
   refs: credential = apiKey, signing = secret, passphrase = passphrase.
4. **Import seam (engine).**
   - `Cx::request_import(context, method, url, step, ImportSpec)` requires a
     live session-owned Read context whose credential is the primary slot
     with `CredentialRole::SessionAuth`. Anything else, including Command
     and Background contexts, is `Invalid`.
   - Before routing, the engine checks the exact Context, attempt and
     generation. A retired session has already discarded the managed entry,
     so this check is the second fence.
   - A 2xx body accumulates in a pre-reserved, capped zeroizing buffer
     (fail closed over the cap) and goes to the provider import. It never
     reaches `on_response`, `account_auth_outcome`, evidence or logs.
   - Limits:
     - Body arrival is zeroized on the engine's `HttpChunk` path. A
       host-delivered whole `Event::HttpResponse` body is a host-owned
       `&str` the engine cannot wipe.
     - For host providers, `Action::Import { body }` carries a copy across
       the host boundary (redacted from Debug).
     - Both limits are stated as best effort, matching the JS
       string/decompression buffers.
   - `Venue::on_import(pending, Result<ImportReceipt, VenueError>)` replaces
     `on_response` for import attempts only; transport loss stays on
     `on_request_failed`. A 2xx carrying an error envelope is a provider
     `Refused`, so only non-2xx statuses reach the venue classifier.
   - The engine records each receipt's refs per (slot, generation) and
     checks membership before signing. It does not parse ref names. A
     stale-generation recipe refuses `Unavailable` before sending.
5. **Provider and host.**
   - `Signer::import -> ImportFuture` plus `import_ready` for the local
     provider. Host providers get `Action::Import { attempt, slot, generation, spec, body }`
     and `Event::Imported` / `Event::ImportFailed`, fenced by attempt id, with
     a host.mjs `import` callback using the same timeout and cancel as
     primitive signing. Host providers implement `import` and drop
     together or not at all.
   - The local provider installs atomically (all refs or none) from bounded
     JSON. The secret is kept as the exact received text; HMAC decodes it at
     sign time through a declared lenient base64url encoding (either
     alphabet, optional padding, as `l2_signature` does today), so the WS
     `Present` reproduces the exact string.
   - Duplicate or late receipts install nothing.
   - The apiKey is a non-secret identifier. It is installed and echoed
     because the venue needs it for the `owner` order field and the
     `POLY_API_KEY` header. It is never a subject, a `stable_id` or a nonce
     key.
   - Drop: claude-55ao7t's `retire_session` calls one hunk this PR adds,
     after the HTTP discard and before the registry revoke. It emits
     `Action::DropImported { generation }` for host providers and drops
     synchronously for local ones. Host providers tombstone a dropped
     generation, so an import promise settling later installs nothing.
     Disposal retires every session through claude-55ao7t's path.
6. **Readiness.** On `on_import` success the venue calls `session_ready`; on
   failure it calls `session_failed` (`NotSent` fan-out), then backs off.
   The derive GET is a session Read with ReadFailure retry inside its 20 s
   bound.
7. **Polymarket recipes and custody** (`venues/polymarket/src/sign.rs`; the
   venue wiring belongs to codex-hcmq3f's managed conversion).
   - ClobAuth: EIP-712 with the `l1Wallet` role, using the cached address.
   - Orders: EIP-712 V2 and ERC-7739. `order_signer` for signature types 0–2
     uses the cached address.
   - L2 HMAC over the imported or supplied refs.
   - The WS frame is built from `Present` values. Those values are transient
     in `SignStep::apply` (the named exception to the canary). Venue
     callbacks and state never hold them.
   - Retired: `Options.key`, `sign::Credentials`, the
     `private_key_address` / `l2_signature` validation in `trading_auth`, the
     second `order_signer` check, `signed_headers_valid`, and the
     `options.key` gates in `start_private` and the `/time` arm. Gates check
     ref presence instead.
8. **Tests.**
   - A canary on every venue callback and venue state, including the auth
     observation.
   - Atomic install and partial-install refusal.
   - Duplicate receipts, and late arrival after retirement for both the
     local provider and a host promise settling after `DropImported`.
   - Exact Context/attempt/generation checks and read delivery.
   - Stale-generation refusal.
   - Address and v-mapping vectors.
   - Padded (`AAA…=`) and unpadded (`-__-AQ`) secret fixtures.
   - The supplied-L2 path.
   - Exact WS frame bytes.
   - In this PR, test-venue equivalents of placement and all signature
     types reproduce the existing vectors through refs. The real trading
     tests 161 and 441 move to refs in codex-hcmq3f's conversion.
   - The #1882 REDs depend on the Plan seam; P6b does not claim them.
   - Hosted/keyless import is refused.

**Owner conditions recorded.**
- claude-m26ps4: primary slot with `CredentialRole::SessionAuth` and binding
  there; the echo is non-authoritative.
- claude-55ao7t: the retirement hunk, the readiness calls, and both fences.
- codex-uufub4, source gates:
  - Import and drop authority is the full tuple (engine issuer, identity or
    subject, slot, session generation, exact attempt). A numeric slot plus
    generation alone collides across accounts.
  - The tuple is rechecked on async commit, and a stale install is wiped and
    disposed atomically.
  - Typed `on_http_failed` remains for transport and expiry causes.
  - The import body never enters evidence, logs or the venue, even on parse
    failure.
  - A provider refusal or timeout produces exactly one terminal outcome.
- codex-uufub4: import stays bound to trusted session-generation authority,
  never exposes material, never infers identity or nonce, and grants no
  entry loan from late callbacks.

No merge without a window.
