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
   `{t:'sign-req', d:{id, type:'op', v:2, exchangeName, accountName, op}}`.
   - `op` = `{algorithm, slot: 'primary', key: 'signing' | ExtraKey, payload: base64, receipt}`.
   - `payload` is the fully resolved exact message or digest. The server
     resolves `bytes` and `credential` parts before sending. A `result` part
     means a multi-operation batch, which the server sends as an ordered
     `ops` array; each later operation names its `result` index and encoding.
   - It never reuses `http` or `ws-auth`. An old browser would HMAC a reused
     type with `secret` and return a wrong signature.
2. **Response shape.**
   `sign-res d: {id, v: 2, values: base64[]}` or `{id, v: 2, error}`.
   Widths are validated per algorithm (`SignRequest::validate_result`). A v2
   request answered without `v: 2` is an error.
3. **Capability negotiation.** The browser advertises
   `signOps: {v: 2, algorithms: [...], slots: [...]}` in `InitPayload`, which
   is included in `getInitFingerprint`.
   - The server sends `type: 'op'` only to a session that advertised that
     algorithm. Otherwise the Rust provider refuses before sending
     (`unsupported` / `notSent`). That refusal never reaches the wire, so it
     cannot feed the fatal streak.
   - An old server drops the unknown field. An old browser never advertises,
     so it never receives v2.
4. **Browser executor.** The answer comes from the same owned local provider
   that dispatch uses, never from TS venue classes.
   - `RustEngineExchange.signOperation(op)` → WASM `hostedSigning({kind: 'op', …})`.
   - That extends `registry/src/hosted_signing.rs` `Input` with
     `Op {algorithm, key, payload}`. It is admitted only for the account's
     declared family recipes and slot.
   - It reuses `LocalSigner::sign_now`, including role binding of extra refs.
   - The deadline cap is raised from the current 5 s to sit inside the
     server's 15 s.
5. **Algorithms.** `ed25519`, `es256`, `secp256k1Digest`, `starkDigest`,
   `zkLinkContract`, and Lighter Schnorr after P3. Each family's adoption
   enables only its own algorithms in `construction-rules.json`
   `remoteSigning`.
   - Coinbase's existing `coinbase-jwt` path stays for TS-hosted accounts
     until its Rust family adopts `es256` ops.
6. **Failure semantics.**
   - A provider refusal or timeout before release is `notSent`.
   - A late or duplicate `sign-res` is fenced by id and attempt, as with
     `ws-auth`.
   - No HMAC format, `extraData` or payloadSigner scheme changes.

### tealstreet-next changes (for its owners; not made here)

| Area | Change |
| --- | --- |
| `packages/safe-cex/src/types.ts` (copy) | `type: 'op'`, `v`, `op`/`ops`, `values` fields; consume the SDK release that adds them. |
| `services/exchange-server/src/server.ts` | Accept and forward `signOps` in `InitPayload` and `getInitFingerprint`. Pass it to the Rust account construction as the session capability. |
| `services/exchange-server/src/remote-signer.ts` | No protocol change. Confirm v2 errors are counted like any `error`, and that refusals before sending never reach the wire. |
| `apps/web/.../ExchangeServerWorker.ts` | Advertise `signOps` from the loaded SDK. Route `type: 'op'` to the signing exchange (`signingHost`), not to `handleSigningRequest`. |
| `signingHost.ts` | Register accounts whose signing material lives in `key`, `auth` or `omniKeySeed` too. Answer `op` through `exchange.signOperation`. |
| `exchangeAccountController.ts` | Add a third arm for `type: 'op'` on the main-thread path. |
| Custom-script sandbox (`PostMessageRemoteSigner.ts`) | Unchanged. It may opt in later. |

Open item for the NX owners: Paradex and Polymarket keep signing material in
`key`, which is in `INIT_FIELDS`. Hosted accounts may already send it to the
server. This needs verification, independent of this plan.

### Proof

Both halves need tests for each adopting family:
- **Server half:** a keyless Rust account with an injected fake
  `remoteSigner` that records `op` requests. Assert the exact payload bytes
  and refusal when the capability is not advertised.
- **Browser half:** WASM `signOperation` with a local setup. Assert exact
  vectors.

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
   - On a 2xx response the engine passes the body bytes straight to
     `Signer::import(ImportRequest {slot, generation, spec, body})`. It never
     calls `on_response` with the body.
   - The provider parses, validates, decodes and installs refs bound to
     `(slot, session generation)`, and returns the ref names plus echoed
     non-secret fields. The engine then calls the venue's
     `on_import(pending, Result<ImportReceipt, VenueError>)`.
   - A non-2xx or unparseable body is a typed failure: status and the venue
     code are kept, and secret-shaped fields are never echoed.
4. **Retirement hook.**
   - `Signer::drop_imported(slot, generation)` is called by
     `retire_session` (claude-55ao7t) and on engine disposal.
   - Recipes that reference a retired ref fail with `Unavailable`, and the
     session core re-derives.
5. **Provider surface.**
   - The local provider implements `import`.
   - Injected JS and Workers providers get an optional `import` callback; if
     it is absent, the import refuses `unsupported`.
   - Keyless remote import is out of scope: hosted Polymarket stays
     unsupported until P8 adds a remote L1 op. Its derived key would then live
     on the device.
6. **Ownership.**
   - Engine changes (`Cx::request_import`, response routing, the
     `Signer::import` and `drop_imported` trait methods) need uufub4 sign-off
     for signing/dispatch and m26ps4 sign-off for credential semantics.
   - claude-55ao7t calls the retirement hook from the session core.
   - codex-hcmq3f wires the Polymarket continuation.

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
