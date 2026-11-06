# Non-HMAC and unassigned signer migrations

Owner: claude-19pt8s (overseer claude-ddi2il). Base: landing `fa5d5fb90c4`.
Owner rule (Sam, 2026-10-22): signing happens when the request is sent, at one
choke point, with a host-supplied signer (local, remote, enclave or parent).
Venues hold no secrets. Remote signing must keep working: browser
`signHttpRequest`/`signWsAuth`, the keyless `remoteSigner` and the existing
sign-request/sign-response wire. This document is a plan, not proof that any
family has migrated.

Scope is 14 families: ApeX Omni, AsterDEX (V1/V3), Bitunix, Crypto.com,
Orderly, Backpack, Coinbase, Hyperliquid (+spot), TxFlow, GRVT, Polymarket,
Paradex, Lighter and BreakoutProp. Bitunix and Crypto.com were confirmed as
this lane's by claude-ddi2il. #1718 (codex-5c57jl) owns Bybit, OKX, Bitget,
BloFin, MEXC, Phemex, WOO, Bitrue, BingX, KuCoin, Kraken and WEEX; Binance is
the landed reference.

## Shared contract consumed

Families consume the landed [signer core](RUST_SIGNER.md),
[prerequisites](RUST_SIGNER_PREREQUISITES.md),
[setup policy](RUST_SIGNER_SETUP_POLICY.md),
[hosted bridge](RUST_HOSTED_SIGNER_BRIDGE.md) and the remote wire and
payloadSigner contract in [RUST_REMOTE_SIGNING.md](RUST_REMOTE_SIGNING.md).

- **Identities and preparation:**
  - Venues hold opaque `Identity` refs.
  - `SignStep::prepare(&DispatchContext)` builds canonical bytes from the
    protocol stamp and signing entropy at release.
  - `request_signed_managed`/`sign_http`/`send_signed`/`connect_signed` stage
    unsigned work.
  - Every resend re-prepares and re-signs.
  - A failure before dispatch is `notSent` for that physical attempted leg only.
    Command aggregation keeps any earlier released, accepted or unknown legs;
    a later signer refusal cannot erase them.
- **Provider today:** the local provider (`crypto/src/signer.rs`) implements
  only HMAC-SHA256/512, SHA256 and SHA256(secret-suffix).
  - `Ed25519`, `Es256`, `Secp256k1Digest` and `StarkDigest` are declared but
    return `Unsupported`.
  - `SchnorrDigest` is reserved and declared with a 64-byte result
    (`engine/src/signing.rs:314-319`), but Lighter signatures are 80 bytes.
  - No algorithm exists for zkLink.
- **Identity shape:** `Identity` carries `credential`, `signing` and an
  optional `passphrase` ref, plus the trusted optional `stable_id`. It has no
  slot for additional signing keys. `LocalIdentity` accepts the trusted
  `stableId` and rejects unknown fields.
  - Families that need an identity or nonce scope keep `require_stable_id()`.
    A missing id is a typed unavailable, never a key or address hash.
- **Setup encodings:** `secretEncoding` already decodes Utf8, Base64, Hex and
  Pem (`crypto/src/signer/key_material.rs`).
- **Engine policy:** `RequestContext` already owns `RetryPolicy` and `Pacing`
  (`engine/src/request_context.rs:91-130`).
- **Remote wire:** half 2 is keyless options (`remoteSigner` or
  `payloadSigner`), as in RUST_HOSTED_SIGNER_BRIDGE.md, and is primary-only.
  - It does not re-run HTTP config signing: `remoteSigner` carries exact
    preimages in the existing ws-auth envelope.
  - The base signer handles `hex`, `base64`, `hex-base64`,
    `apexomni-base64` (HMAC) and `bitunix-double-sha256` (double SHA256)
    (`base.ts:2337-2377`).
  - `coinbase-jwt` works today only through the kept TS Coinbase override
    (`rust.exchange.ts:430-441` → `coinbase.exchange.ts:142`), which mints the
    whole JWT (nonce and claims) instead of signing an engine preimage.
  - payloadSigner covers only `hmac-sha256` and `stark`.

## Inventory

Counts are frozen ratchet entries at the base: `signing-legacy-sites.json`
(S) and `request-legacy-sites.json` (R). "Eager" means signed before engine
release. "Venue send" means signed inside the venue just before `cx.request`;
that is still before engine release, so it is not the choke point either.

| Family | Primitive and key material | Where it signs today | Nonce, time and session | S/R |
| --- | --- | --- | --- | --- |
| ApeX Omni | REST and WS login: HMAC-SHA256 base64, keyed by the base64 text of the secret, plus key and passphrase headers. Orders: zkLink Rescue/MuSig over a 36-byte contract, seeded by a 65-byte `omniKeySeed`. | REST: `lib.rs:112 send_signed` → `sign.rs:95` at venue send. zkLink: eager in `trading.rs:292 prepare_order` (`:369`, `:469`), every lot signed before the first write. WS: `account.rs:645`. | Wire stamp; the legacy stamp is floored to seconds. One legacy resend on a signature rejection reuses the zkLink body. Up to 3 read retries. | 13/45 |
| AsterDEX V1 | HMAC-SHA256 hex over the raw query. | `request.rs:181 dispatch_signed`: venue send with the protocol stamp. GET uses the secondary identity; writes use the primary. | One `-1022` resend; a `-1021` resync then one resend. listenKey comes from a signed POST/PUT. | 28/71 |
| AsterDEX V3 standard / Code | secp256k1 over `eip191(keccak(abi(payload,user,signer,nonce)))`. Code endpoints use EIP-712 `Message(string)`, chain 1666/714, and are not URL-encoded. | `sign.rs:141 Query::signed` (venue send). | The nonce comes from a process-global static map per signer address, `max(now·1000, prev+1)` (`sign.rs:231`). The `-1022` signature resend is V1-only, but the `-1021` Timestamp clock resync and one resend apply to V3 as well (`request.rs:276`). | (above) |
| Bitunix | `hex(sha256(hex(sha256(nonce‖ts‖key‖query‖body))‖secret))`, the `Sha256SecretSuffix` shape. | `account.rs:281 signed_request` at venue send. GET uses the secondary identity. WS login (`account.rs:581`) uses the trimmed primary. | UUID nonce from `fill_signing_entropy` plus a wire stamp. 2 s login retry. | 25/23 |
| Crypto.com | HMAC-SHA256 hex over `method‖nonce‖key‖params‖nonce`; key and sig go in the JSON body. | `account.rs:420 signed` at venue send, always primary (`read_credentials` is validated but unused). WS `public/auth` at `account.rs:779`. | Nonce and `id` are the wire stamp in ms, so they collide within one millisecond. One 40101 resend. | 14/35 |
| Orderly | Ed25519 over `{ts}{METHOD}{path}{body}`, base64url with padding. The seed is base58; the public `orderly-key` is supplied. | Eager at issue: `account.rs:334`, `history.rs:451`, `trading.rs:1106`. WS auth at `account.rs:1395`. | Wire stamp; the WS stamp is `now as u64`. | 12/39 |
| Backpack | Ed25519 over `instruction=…&timestamp&window`, base64; the seed is base64. | Eager in `account.rs:300 signed`, which re-decodes the secondary seed on every GET. `trading.rs:589` batch. WS SUBSCRIBE at `account.rs:758`. | Wire stamp; windows are 60000 (REST) and 5000 (WS). | 19/32 |
| Coinbase | JWT per request: ES256 (P-256 PEM/SEC1/PKCS8) or EdDSA (base64 seed, CDP JSON). The signature covers `b64(header).b64(claims)`. | `rest.rs:53 dispatch` at the venue's paced pop. The secondary identity is used for account/history GETs. WS: one JWT per channel subscribe (`account.rs:1031`). | `nbf = floor(now/1000)`, `exp = +120`, UUID nonce from signing entropy. | 11/34 |
| Hyperliquid (+spot) | secp256k1 over the EIP-712 phantom-agent digest `keccak(msgpack(action)‖nonce‖vault)`. | `trading.rs:573 pump_trades` at venue send, plus `schedule.rs`. Addresses are derived from the secret (`account.rs:315`, `lib.rs:1077`). | Process-global `nonce::ethereum_agent`, keyed by sha256 of the address and shared with TxFlow. No write retries. | 18/52 |
| TxFlow | secp256k1 over the EIP-712 `Agent` digest from HL's `action_hash`. | `trading.rs:616 pump_trades` at venue send. `lib.rs:51` derives the address. | Shares the HL agent nonce domain. | 10/30 |
| GRVT | secp256k1 over the EIP-712 `OrderWithBuilderFee` digest. The body carries the signer address derived from the secret. Login presents the API key in the body. | Eager at plan time: `trading.rs:747 placement`, and amend replacements in `trade_response` (`:1633`). | u32 nonce from `keccak(uuid(signing entropy))`; expiry now+1 h. Session cookie from login. A re-login on code 1000 resends the same signed body. | 14/56 |
| Polymarket | Order: EIP-712 V2 (+ERC-7739 wrapper). L1: `ClobAuth` EIP-712. L2: HMAC-SHA256 base64url, keyed by the base64url-decoded API secret. All derive the address from `key`. | Eager: `trading.rs:226 placement_body`, and `account.rs:259 signed_request`. L1 derive at `account.rs:788`. | L1 uses server `/time`; L2 is in seconds. Order salt from signing entropy. **WS sends raw key/secret/passphrase** (`account.rs:286`). Derived L2 credentials are held in the venue. | 24/35 |
| Paradex | Stark ECDSA over a Pedersen typed-data hash (fits `StarkDigest`). An Ethereum key derives the Stark key via secp256k1 over `Constant("STARK Key")` plus grinding (needs `l1_chain_id` from `/system/config`). | Eager: `/auth` at `account.rs:476`; orders in `trading.rs:214` (plan time); amend at `:1167`. Existing delegated `payloadSigner` via `cx.sign_request` (`account.rs:416-500`). | Auth `exp` is +300 s; `signature_timestamp` is ms. The JWT is held in the venue and refreshed at 180 s; WS auth uses the bearer. | 8/62 |
| Lighter | ECgFp5 Schnorr over Poseidon2 Goldilocks words, 80-byte signature. Hedged nonce from signing entropy. | Eager: the auth token on every use (`account.rs:732`); transactions in `trading.rs:503` before `sendTx`. | The tx nonce comes from server `nextNonce` plus a local increment; code 21104 refetches and re-signs once. Auth deadline now+60 s. | 13/51 |
| BreakoutProp | No crypto: username/password login body, then the `DXAPI <token>` header. | `lib.rs:386` login body at dispatch; the header is added at `lib.rs:340`. | Session token with expiry; one re-login on 401. | 10/24 |

Existing remote/hosted paths that must keep working:

- TS-hosted Coinbase and Orderly (`hostedSigningVenues.ts`).
- Paradex `payloadSigner` (engine `cx.sign_request`).
- The ApeX `apexomni-base64` WS output format, which is refused for Rust today.
- Bitunix's `bitunix-double-sha256` WS format.
- Lighter's read-only `remote_signer` flag.

Bitunix is also the shared "unmigrated control" in:

- `registry/tests/dispatch_provider.rs:445`
- `registry/tests/hosted_signing.rs:227`
- `hosts/wasm/src/lib.rs:1453`
- `rust.hosted-signing-venues.wasm.test.ts:134`

Migrating Bitunix moves those controls to a still-unmigrated family.

## Shared prerequisites (provider choke point)

Each item below is one shared PR against engine, crypto, registry and host
files. Coordinate it before opening:

- codex-uufub4 owns the signing core and dispatch.
- claude-m26ps4 owns the nonce and credential subject.
- codex-5c57jl serializes registry, construction, SDK, host and ratchet edits
  for the HMAC families.
- Socket auth is in #1771.

No family source edit depends on an unlanded prerequisite.

- **P0. Named identity key slots.** Add named extra signing refs to an
  identity, all held by the provider. The signing-core owner (codex-uufub4)
  and the credential owner (claude-m26ps4) review it. Needed for:
  - the ApeX zkLink `omniKeySeed`, alongside the HMAC key, secret and
    passphrase;
  - the Polymarket L1 wallet key, alongside the L2 API key, secret and
    passphrase.

  Schema conditions (2026-10-23):
  - Sign-offs: uufub4 owns placement in `signing.rs`/`signer.rs`; m26ps4
    owns nonce and credential semantics.
  - Extra refs are signing material only. They are never a nonce domain, an
    account subject, or a source or fallback for a stable id, account or
    slot.
  - Extra refs travel inside the selected slot's `Identity`. There is no
    per-request selector beyond what the algorithm needs.
  - Bounds apply to entry count, total name and ref bytes, and each entry.
  - Trusted setup rejects duplicate or undeclared names.
  - Every algorithm validates exact key, payload and result lengths.
  - WASM/JS/Workers schema gates change together.
  - The reviewed schema pin is frozen and sent before consumers integrate.
- **P1. Setup key encodings.** Extend the existing `KeyEncoding` and
  `secretEncoding` setup seam; add no parallel mechanism.
  - New decodings: Base58 (Orderly seed), Base64Url (Polymarket L2 secret)
    and CDP JSON (Coinbase).
  - New forward transform: encode-to-base64-text, for the ApeX HMAC key.
  - Per-family defaults are declared in
    `packages/safe-cex/src/exchanges/rust/construction-rules.json`.
  - Already covered: Backpack base64, hex keys (Lighter, Stark, secp256k1)
    and Coinbase PEM.

  No transform happens in a recipe.
- **P2. Concrete local primitives.** Implement `Ed25519`, `Es256`,
  `Secp256k1Digest` and `StarkDigest` in the local provider, reusing the
  existing `safe_cex_crypto` implementations. This adds no dependencies.
  - Each comes with exact result and encoding contracts and independent
    vectors.
  - Each is mirrored in the WASM/JS/Workers provider types.
- **P3. New algorithm contracts.** These are schema changes, reviewed first by
  codex-uufub4 and claude-m26ps4.
  - `ZkLinkContract`: exact 36-byte payload, seed ref, 64-byte result.
  - Lighter Schnorr: change `SchnorrDigest`'s result length to 80 bytes and
    update its WASM/JS mirrors, or add a separate variant.
    - Its preimage is the Poseidon2 word payload.
    - Its hedge bytes are drawn by the engine at preparation
      (`DispatchContext.signing_bytes`) and passed in the payload. They are not
      provider-internal randomness, which keeps tape-clock signatures exact.
- **P4. Public derivations.** A non-secret provider operation that returns an
  identity's public key or address, for:
  - the Hyperliquid, TxFlow, GRVT and Polymarket addresses;
  - the Paradex Stark public key and account address;
  - the Lighter public key.

  The value is fetched once at setup or startup and is never derived from a
  key in a venue. Paradex's Ethereum-to-Stark derivation becomes a
  trusted-setup derivation that produces a provider-held key ref. It needs
  `l1_chain_id`, so its sequencing is an open question for P4 review.
- **P5. Nonce domains (claude-m26ps4).**
  - The Aster V3 per-signer static map and HL/TxFlow `ethereum_agent` move to
    the shared nonce core, keyed by a non-secret domain key: setup `stableId`,
    or a public identifier when the venue's own sequence is public (Aster V3:
    the configured signer address). The key is never a hash of key material,
    and never an address derived from a key inside a venue.
    - HL and TxFlow keep sharing one sequence per public agent address. Until
      their signer migration supplies that address from setup (P4
      `PublicAddress`), #1852's `reserve_agent_nonce` still derives it inside
      the venue from the raw secret. That is a listed interim, removed with the
      migration.
  - Draws happen at preparation and stay process-wide monotonic.
  - Lighter's tx nonce is server-sequenced. The engine owns the allocated
    counter.
    - A re-preparation of the same transaction reuses its allocated nonce
      unless the venue refused that nonce (21104), which forces a refetch.
    - This is the one stated exception to "every resend draws fresh".
- **P6. Session credentials.** Session tokens move to the shared WS-auth and
  session boundary (session mutations: claude-55ao7t). Families migrate the
  signed acquisition request and track one token follow-up each, with no new
  venue caches. The tokens are:
  - Paradex JWT;
  - GRVT cookie;
  - BreakoutProp DXAPI token;
  - Aster listenKey.
- **P6b. Derived signing keys (Polymarket L2).** The L2 secret is an HMAC
  signing key derived from a server response at runtime, so it is not a
  session token.
  - It needs a shared provider operation that imports a response field under a
    provider-held ref, without the venue seeing the bytes.
  - Polymarket is blocked on an owner decision here.
- **P7. Hosted halves.** Every family (implicit in every row) adds its recipe to the shared
  `hosted_signing` adapter (half 1, browser `signHttpRequest`/`signWsAuth`)
  and keyless `remoteSigner` admission (half 2). Each migrated family gets
  one positive test per half; no TS venue classes.
- **P8. Remote wire formats.** No half-2 shape exists today for:
  - Ed25519 (Orderly, Backpack; REST and WS);
  - ES256/EdDSA over an engine-built JWT preimage (Coinbase REST and WS;
    today's `coinbase-jwt` relies on the TS class minting its own JWT);
  - secp256k1 digests (Aster V3/Code, Hyperliquid, TxFlow, GRVT,
    Polymarket);
  - zkLink (ApeX orders);
  - Lighter Schnorr.

  Adding formats to the envelope or payloadSigner schemes is a cross-repo wire
  change that needs owner approval and NX coordination. Until it lands, such
  a family's keyless half 2 stays an explicit typed refusal, never a TS
  fallback.

  Half 2 possible on today's wire:
  - Bitunix (`bitunix-double-sha256`);
  - Crypto.com, Aster V1 (`hex`);
  - ApeX REST and WS login (`apexomni-base64`);
  - Paradex (payloadSigner `stark`).

  Half 2 is primary-only. Secondary-identity reads (Aster V1 GET, Bitunix
  GET) under keyless options keep the existing typed refusal of secondary
  remote signing.

## Family order (ranked by what blocks RequestContext lanes)

One family per PR. The exception is Hyperliquid with TxFlow, which share one
nonce domain and one action hash. Shared prerequisites land first as their
own PRs. Every family PR covers:

- adopting `SignStep`/`KeyRef` and signing at dispatch;
- deleting the venue's secret fields and eager signing;
- keeping the old signer as a `cfg(test)` oracle with byte-identical
  local-provider proof;
- pruning its own ratchet entries and scanning its sign module;
- a positive remote test per half (half 2 only once P8 covers the format);
- never relabelling signed work as public;
- declaring the actual `SignReceipt` stamp and expiry used.

A family is migrated whole. There is no partial stage (for example a
V1-only Aster PR) before its migrated construction path is enabled.

**Retries and pacing.**
- Resends, resyncs and pacing are `RequestContext` `RetryPolicy`/`Pacing`
  work, owned by each venue's RequestContext lane.
- The signer PR adds no venue retry counter or queue.
- Where the RequestContext adoption has not landed, the signer PR preserves the
  existing observable contract through the engine's re-prepare path. The
  existing counters stay frozen for that lane to delete. It does not
  re-implement them.
- Today `RetryPolicy` has only `Never`, `ReadFailure` and
  `DefinitiveRefusal`, which cannot express prerequisite-then-resend:
  resync-then-resend, re-login-then-resend and refetch-nonce-then-resend.
  That variant is a shared request-core extension (codex-uufub4, with
  claude-m26ps4 for the nonce case). This lane reports it as a dependency and
  never adds a per-venue substitute.
- The affected sites, all for the RequestContext lane to retire:
  - ApeX signature-rejection resend versus 3 network retries;
  - Crypto.com 40101;
  - Aster -1022/-1021;
  - Lighter 21104;
  - GRVT/BreakoutProp re-login;
  - Coinbase `rest.rs` pacing queue.

| # | Family | Waiting lane | Needs | Receipt (stamp / expiry) | Notes |
| --- | --- | --- | --- | --- | --- |
| 1 | ApeX Omni | codex-hcmq3f | P0, P1, P3 zkLink, P7; P8 for keyless orders | wire or legacy-second stamp; none | Shared PR first: P0 seed slot, P1 base64-text transform and `ZkLinkContract`. The family PR moves REST, WS login and zkLink to release, signing each lot or leg. Keep legacy timestamp/body quirks, the klines exception and native zkLink packing. The hcmq3f `lib.rs` regions (api_response, `Pending::Candles`) are reserved. Until P8, keyless ApeX order placement is refused (typed `unsupported`/`notSent`); keyless REST reads and WS login work. |
| 2 | AsterDEX | codex-ih35js | P2 secp256k1, P5; P8 for keyless V3 | protocol stamp; none | V1, standard and Code migrate together. Keep the codec distinctions, native query/preimage bytes, the GET secondary, primary writes, shared stamps/nonces, auth inference and builder attribution. Source in `sign.rs`/`request.rs`; `account.rs` belongs to #1785-1788 and `lib.rs`/`tests/trading.rs` to #1799. |
| 3 | Bitunix | codex-9czdjh | none | wire stamp; none | Fits today as `Sha256(Credential)` → `Sha256SecretSuffix(Result hex)`, tested at `crypto/src/signer.rs:870-905`. WS login is staged with #1771's socket core. Move the unmigrated controls to another family. |
| 4 | Crypto.com | codex-ih35js | none | ms nonce; none | Fits today as HMAC with credential interpolation, plus `Present` into the body. Keep the ms nonce and `id`. #1788 owns account/trading files. |
| 5 | Orderly | codex-hcmq3f | P1 base58, P2 Ed25519, P8 | wire stamp; none | Retire the TS-hosted Orderly kept path only after the Rust positives pass. |
| 6 | Backpack | codex-hcmq3f | P2 Ed25519, P8 | wire stamp; none (window is a duration, as with Binance `recvWindow`) | |
| 7 | Coinbase | codex-hcmq3f | P1 CDP JSON, P2 Es256/Ed25519, P8 | `nbf`; `exp` = +120 s (local and half-1 provider; today's `coinbase-jwt` remote path picks its own claims) | The JWT is built at release with the entropy nonce. Retire the TS-hosted Coinbase path only after the positives pass. |
| 8 | Hyperliquid + TxFlow | codex-zs5e83 (HL); codex-hcmq3f (TxFlow), coordinating the joint nonce/action-hash proof | P2, P4, P5, P8 | nonce stamp; none | Digest recipes; the nonce is drawn at preparation. |
| 9 | GRVT | codex-zs5e83 | P2, P4, P6, P8 | none; +1 h | Signs at release, including amend replacements. Re-login re-prepares and never resends the stale signed body. #1785-1788 own `account.rs`. |
| 10 | Paradex | codex-zs5e83 | P2 Stark, P4, P6 | auth `exp` +300 s; order ms stamp (half 2 on the existing `stark` payloadSigner) | Map the payloadSigner contract onto the shared provider and hosted adapter. Delete the Paradex-only engine path (`Cx::sign_request`, `PendingSignature`, `SignatureTarget`, `engine.rs:145-155,1715`) in the same PR. #1787 owns account/trading files. |
| 11 | Lighter | claude-m26ps4 (P3 Schnorr recipe, [plan](RUST_LIGHTER_SCHNORR_RECIPE.md)); codex-zs5e83 (venue P4/P5/P8 integration) | P3 Schnorr, P4, P5, P8 | none; auth +60 s, tx +10 min | The auth token is minted per use at release. Nonce reuse follows the P5 rule. |
| 12 | Polymarket | codex-hcmq3f | P0, P1, P2, P4, P6b, P8 | L2 seconds; none | Blocked on the P6b owner decision. The raw-credential WS frame becomes `Present` fields. |
| 13 | BreakoutProp | codex-hcmq3f | P6 | none | Password `Present` into the login body; the token follows P6. |

Rows 3 and 4 need no shared prerequisite. They can proceed alongside ApeX's
shared PR once their RequestContext owners agree file windows.

## Proof and gates per PR

- Independent correctness and adversarial review, answering the architecture
  gate: shared provider/setup/nonce choke points, venue canonicalization
  only, no per-venue custody, timing, retry or outcome logic.
- Focused `cargo test -p` for the venue plus engine, crypto, registry, host
  and Binance, with `CARGO_BUILD_JOBS=2`, plus SDK tests for the files
  touched. CI owns WASM, size, timing and replay.
- Delayed, refused, timed-out and stale-provider regressions.
- Re-sign on every permitted resend.
- Unknown writes are never replayed.
- Exact signatures under immediate local providers and tape clocks.
- No merge until claude-ddi2il grants a window.
