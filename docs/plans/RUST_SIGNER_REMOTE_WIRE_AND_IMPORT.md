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
- **SDK guard (approved 2026-10-25; SDK source checkpoint).** Sam's
  overseer ruled to refuse hosted Rust material now, before hosted-leases
  hardening. A hosted account is selected explicitly by `hostedMode` or
  `remoteSigner`; key length and scalar range never select its mode.
  - Paradex: without a configured `payloadSigner`, a nonempty `key` is private
    by its existing field contract and refuses. With `payloadSigner`, `key` is
    public subkey metadata, including the same 66-character felt. Its public
    and private forms cannot be distinguished by parsing.
  - Polymarket: any nonempty `key`, `polymarket.apiSecret` or
    `polymarket.apiPassphrase` refuses. The API-key id, funder address,
    signer type and market slugs are public metadata.
  - Both families refuse nonempty `secret`, `passphrase` and `starkPrivateKey` in hosted
    mode, including explicit public-only accounts. Empty fields are absent;
    whitespace is nonempty material. Paradex account/subkey addresses stay
    public. Local construction and payload-callback-only local construction
    keep their existing contract.
  - Refuse synchronously before `BaseExchange` retains options, WASM loading,
    provider invocation or I/O, using `invalidInput` / `notSent` and a fixed
    message that never echoes the material. Never fall back to a local key,
    another callback or another engine. This enables no new hosted recipe.
  - Family rows in `construction-rules.json` declare private fields and the
    payload-callback key contract. The shared constructor applies them
    generically. The native table compiler validates their closed field grammar
    but emits exactly the same native runtime policy.
  - Reviewer: claude-m26ps4, shared setup/field-contract direction. The original
    dedicated target executed 38 controls: 26 intended assertion RED and
    12 GREEN on unchanged runtime at `6d13105b9b4`; no setup or compile failures.

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
   - Its address arrives through P4's one channel: `fetch_public` for a
     local provider, or bound account metadata for host/remote. There is no
     other path. The venue never derives it.
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

### P6b part 1: the `l1Wallet` role and its address

Plan, 2026-10-24. Owner claude-19pt8s, stacked on #1963 (P4). Reviewers:
codex-uufub4 (engine) and codex-hcmq3f (consumer; the Polymarket
conversion, #1882, adopts it).

**Scope.** Engine and crypto only. No Polymarket venue edits, and no
construction-rules row for Polymarket; the conversion adds its admission.
Keyless/hosted Polymarket stays unsupported (section 5 above), so only the
local provider answers. There is no host/remote metadata path in this part.

**Role table** (engine `signing.rs`). This is one closed table: each
`ExtraKey` names exactly one algorithm.
- `ZklinkSeed → ZkLinkContract`: no public form, as today.
- `L1Wallet → Secp256k1Digest`: name `l1Wallet`; the raw 32-byte wallet
  scalar, declared hex by family policy (`ExtraKeyPolicy`).

**Provider** (`LocalSigner`).
- A role ref is usable only through its role's algorithm, by `Sign` and by
  public operations alike. An `l1Wallet` ref answers `PublicAddress` and signs
  `Secp256k1Digest` (the later ClobAuth recipe).
- The ref is refused everywhere else:
  - `Present`;
  - a `Credential` payload part;
  - HMAC/MAC algorithms;
  - `PublicKey` of any algorithm;
  - `ZkLinkContract`.
- A `zklinkSeed` ref keeps its existing rules and answers no public operation.
- An ordinary `Sign` keeps today's behaviour: an untyped ref still signs
  `Secp256k1Digest` until the cross-family key-typing follow-up.
- The role is the declaration. A role ref needs no `signing_algorithm`, and
  `insert`/`remove` keep the role semantics they have today.

**Address channel.** P4's `fetch_public` with
`PublicAddress { key: identity.extra[l1Wallet] }` at trusted setup. The
venue never derives it. The result is the *actual* L1 signer. It never
becomes the account subject or `stable_id` by itself (condition 6). The
funder, maker and imported L2 API identity are distinct values, and none is
compared with it as if it were the wallet.

**Codegen and TS.** `build_support` `ExtraKey` and the TS `ExtraKeyPolicy.key`
union gain `l1Wallet`, and `host.d.mts` `SigningIdentity.extra` gains
`l1Wallet?`. Generated policy only; no family declares it yet.

**REDs** (existing decode, shown failing first):
- R1: a setup with policy `extra: {key: l1Wallet, encoding: hex}` and
  `PublicAddress` on the extra ref equals the pinned ethers address. Today
  `l1Wallet` is an unknown variant.
- R2: role isolation. An `l1Wallet` ref is refused for each of `Present`,
  a `Credential` part, `HmacSha256`, `PublicKey{Ed25519}` and `ZkLinkContract`.
  A `zklinkSeed` ref is refused for `PublicAddress` and `Secp256k1Digest`.
  It fails today at decode.
- R3: codegen emits `ExtraKey::L1Wallet` for a rule declaring it. Today the
  rule is refused.

**Tests.**
- `fetch_public` returns the role's address.
- A secondary-slot `l1Wallet` is refused at setup even when policy declares
  it, because import and L1 are primary-only (condition 6). The engine
  enforces this in `apply_to_identities`.
- Required-for-private absent or empty fails setup.
- Disabled `secp256k1` arm: `Unsupported`.

**Later parts** (each planned and reviewed separately):
- 2: the ClobAuth L1 recipe, signed with the `l1Wallet` role.
- 3: `Cx::request_import` and `Signer::import`/`drop_imported` (sections 2-7).
- 4: the Polymarket conversion, codex-hcmq3f.

**Architecture gate.** Is this a per-venue fix of a cross-cutting concern?
No. It adds one shared role table for every extra key and reuses P4's
channel. Polymarket gets no special case in the provider.

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
