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
[setup policy](RUST_SIGNER_SETUP_POLICY.md) and
[hosted bridge](RUST_HOSTED_SIGNER_BRIDGE.md).

- **Identities and preparation:**
  - Venues hold opaque `Identity` refs.
  - `SignStep::prepare(&DispatchContext)` builds canonical bytes from the
    protocol stamp and signing entropy at release.
  - `request_signed_managed`/`sign_http`/`send_signed`/`connect_signed` stage
    unsigned work.
  - Every resend re-prepares and re-signs.
  - A failure before dispatch is `notSent`.
- **Provider today:** the local provider (`crypto/src/signer.rs`) implements
  only HMAC-SHA256/512, SHA256 and SHA256(secret-suffix).
  - `Ed25519`, `Es256`, `Secp256k1Digest` and `StarkDigest` are declared but
    return `Unsupported`.
  - `SchnorrDigest` is reserved and has no contract.
  - No algorithm exists for zkLink.
- **What this plan therefore needs:** most families here need shared provider
  work before their family PR. That work goes into the one provider choke
  point, never into a venue.

## Inventory

Counts are frozen ratchet entries at the base: `signing-legacy-sites.json`
(S) and `request-legacy-sites.json` (R). "Eager" means signed at build or
queue time, before engine release.

| Family | Primitive and key material | Where it signs today | Nonce, time and session | S/R |
| --- | --- | --- | --- | --- |
| ApeX Omni | REST and WS login: HMAC-SHA256 base64, keyed by the base64 text of the secret, plus key and passphrase headers. Orders: zkLink Rescue/MuSig over a 36-byte contract, seeded by a 65-byte `omniKeySeed`. | REST: `lib.rs:112 send_signed` → `sign.rs:95` at venue send. zkLink: eager in `trading.rs:292 prepare_order` (`:369`, `:469`), every lot signed before the first write. WS: `account.rs:645`. | Wire stamp; the legacy stamp is floored to seconds. One legacy resend on a signature rejection reuses the zkLink body. Up to 3 read retries. | 13/45 |
| AsterDEX V1 | HMAC-SHA256 hex over the raw query. | `request.rs:181 dispatch_signed`: venue-local signing at dispatch with the protocol stamp. GET uses the secondary identity; writes use the primary. | One `-1022` resend; a `-1021` resync then one resend. listenKey comes from a signed POST/PUT. | 28/71 |
| AsterDEX V3 standard / Code | secp256k1 over `eip191(keccak(abi(payload,user,signer,nonce)))`. Code endpoints use EIP-712 `Message(string)`, chain 1666/714, and are not URL-encoded. | `sign.rs:141 Query::signed` (dispatch, venue-local). | The nonce comes from a process-global static map per signer address, `max(now·1000, prev+1)` (`sign.rs:231`). V3 never resends. | (above) |
| Bitunix | `hex(sha256(hex(sha256(nonce‖ts‖key‖query‖body))‖secret))`, the `Sha256SecretSuffix` shape. | `account.rs:281 signed_request` at venue dispatch. GET uses the secondary identity. WS login (`account.rs:581`) uses the trimmed primary. | UUID nonce from `fill_signing_entropy` plus a wire stamp. 2 s login retry. | 25/23 |
| Crypto.com | HMAC-SHA256 hex over `method‖nonce‖key‖params‖nonce`; key and sig go in the JSON body. | `account.rs:420 signed` at venue dispatch, always primary (`read_credentials` is validated but unused). WS `public/auth` at `account.rs:779`. | Nonce and `id` are the wire stamp in ms, so they collide within one millisecond. One 40101 resend. | 14/35 |
| Orderly | Ed25519 over `{ts}{METHOD}{path}{body}`, base64url with padding. The seed is base58; the public `orderly-key` is supplied. | Eager at issue: `account.rs:334`, `history.rs:451`, `trading.rs:1106`. WS auth at `account.rs:1395`. | Wire stamp; the WS stamp is `now as u64`. | 12/39 |
| Backpack | Ed25519 over `instruction=…&timestamp&window`, base64; the seed is base64. | Eager in `account.rs:300 signed`, which re-decodes the secondary seed on every GET. `trading.rs:589` batch. WS SUBSCRIBE at `account.rs:758`. | Wire stamp; windows are 60000 (REST) and 5000 (WS). | 19/32 |
| Coinbase | JWT per request: ES256 (P-256 PEM/SEC1/PKCS8) or EdDSA (base64 seed, CDP JSON). The signature covers `b64(header).b64(claims)`. | `rest.rs:53 dispatch` at the venue's paced pop. The secondary identity is used for account/history GETs. WS: one JWT per channel subscribe (`account.rs:1031`). | `nbf = floor(now/1000)`, `exp = +120`, UUID nonce from signing entropy. | 11/34 |
| Hyperliquid (+spot) | secp256k1 over the EIP-712 phantom-agent digest `keccak(msgpack(action)‖nonce‖vault)`. | `trading.rs:573 pump_trades` at venue send, plus `schedule.rs`. Addresses are derived from the secret (`account.rs:315`, `lib.rs:1077`). | Process-global `nonce::ethereum_agent`, keyed by sha256 of the address and shared with TxFlow. No write retries. | 18/52 |
| TxFlow | secp256k1 over the EIP-712 `Agent` digest from HL's `action_hash`. | `trading.rs:616 pump_trades` at venue send. `lib.rs:51` derives the address. | Shares the HL agent nonce domain. | 10/30 |
| GRVT | secp256k1 over the EIP-712 `OrderWithBuilderFee` digest. The body carries the signer address derived from the secret. Login presents the API key in the body. | Eager at plan time: `trading.rs:747 placement`, and amend replacements in `trade_response` (`:1633`). | u32 nonce from `keccak(uuid(signing entropy))`; expiry now+1 h. Session cookie from login. A re-login on code 1000 resends the same signed body. | 14/56 |
| Polymarket | Order: EIP-712 V2 (+ERC-7739 wrapper). L1: `ClobAuth` EIP-712. L2: HMAC-SHA256 base64url, keyed by the base64url-decoded API secret. All derive the address from `key`. | Eager: `trading.rs:226 placement_body`, and `account.rs:259 signed_request`. L1 derive at `account.rs:776`. | L1 uses server `/time`; L2 is in seconds. Order salt from signing entropy. **WS sends raw key/secret/passphrase** (`account.rs:286`). Derived L2 credentials are held in the venue. | 24/35 |
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
- `rust.hosted-signing-venues.test.ts:134`

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

- **P1. Setup key transforms.** Add declared per-family secret encodings in
  `construction-rules.json`, applied once in trusted setup (the extension
  point is RUST_SIGNER_SETUP_POLICY.md):
  - ApeX HMAC key = UTF-8 of base64(secret).
  - Polymarket = base64url with padding repaired.
  - Orderly = base58 seed.
  - Backpack = base64 seed.
  - Coinbase = PEM/SEC1/PKCS8/CDP JSON.
  - Lighter = 40-byte hex.
  - Stark and secp256k1 = 32-byte hex.

  Never transform in a recipe.
- **P2. Concrete local primitives.** Implement `Ed25519`, `Es256`,
  `Secp256k1Digest` and `StarkDigest` in the local provider, reusing the
  existing `safe_cex_crypto` implementations. This adds no dependencies.
  - Each comes with exact result and encoding contracts and independent
    vectors.
  - Each is mirrored in the WASM/JS/Workers provider types.
- **P3. New algorithm contracts.** These are schema changes; claude-m26ps4
  and codex-uufub4 review them first.
  - `ZkLinkContract`: exact 36-byte payload, seed key ref, 64-byte result.
  - `SchnorrDigest` redefined as Lighter ECgFp5/Poseidon2: words preimage,
    80-byte result, hedged with provider entropy.
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
    the shared nonce core, keyed by setup `stableId`, never an address or key
    hash.
  - Draws happen at preparation or re-preparation and stay process-wide
    monotonic.
  - Lighter's server-issued tx nonce allocates at preparation from a counter
    the engine owns.
- **P6. Session credentials.** Session tokens move to the shared WS-auth and
  session boundary (session mutations: claude-55ao7t). Families migrate the
  signed acquisition request and track one token follow-up each, with no new
  venue caches. The tokens are:
  - Paradex JWT;
  - GRVT cookie;
  - BreakoutProp DXAPI token;
  - Polymarket derived L2 credentials;
  - Aster listenKey.
- **P7. Hosted halves.** Each family adds its recipe to the shared
  `hosted_signing` adapter (half 1, browser `signHttpRequest`/`signWsAuth`)
  and keyless `remoteSigner` admission (half 2). Each migrated family gets
  one positive test per half; no TS venue classes.

## Family order (ranked by what blocks RequestContext lanes)

One family per PR; the ApeX shared prerequisite is the PR named in row 1.
Every PR covers:

- adopting `SignStep`/`KeyRef` and signing at dispatch;
- deleting the venue's secret fields and eager signing;
- keeping the old signer as a `cfg(test)` oracle with byte-identical
  local-provider proof;
- pruning its own ratchet entries and scanning its sign module;
- a positive remote test per half;
- never relabelling signed work as public.

| # | Family | Waiting lane | Needs | Notes |
| --- | --- | --- | --- | --- |
| 1 | ApeX Omni | codex-hcmq3f | P1, P3 zkLink, P7 | Shared PR first: ApeX key transform and `ZkLinkContract`. The family PR then moves REST, WS login and zkLink to dispatch, signing each lot or leg at release. Keep: legacy timestamp/body quirks, one signature-rejection resend kept distinct from 3 network retries, the klines exception and zkLink packing natively. The hcmq3f lib.rs regions are reserved. |
| 2 | AsterDEX | codex-ih35js | P5 (V3), P2 secp256k1 | V1 fits today. Keep the codec distinctions (V1/standard/Code), native query/preimage bytes, the GET secondary, primary writes, shared stamps/nonces, auth inference and builder attribution. Source in `sign.rs`/`request.rs`; `account.rs` belongs to #1785-1788 and `lib.rs`/`tests/trading.rs` to #1799. |
| 3 | Bitunix | codex-9czdjh | none | Fits today as `Sha256(Credential)` → `Sha256SecretSuffix(Result hex)`. WS login is staged with #1771's socket core. Move the unmigrated controls. |
| 4 | Crypto.com | codex-ih35js | none | Fits today as HMAC with credential interpolation, plus `Present` into the body. Keep the ms nonce and `id`. #1788 owns account/trading files. |
| 5 | Orderly, Backpack | codex-9czdjh (TBC) | P1, P2 Ed25519 | Retire the TS-hosted Orderly kept path only after the Rust half-1/2 positives pass. |
| 6 | Coinbase | codex-9czdjh (TBC) | P1, P2 Es256/Ed25519 | The recipe builds the JWT at release with the entropy nonce and 120 s expiry. Retire the TS-hosted Coinbase path only after the positives pass. |
| 7 | Hyperliquid, TxFlow | codex-zs5e83 | P2, P4, P5 | Digest recipes; the nonce is drawn at preparation. |
| 8 | GRVT | codex-zs5e83 | P2, P4, P6 | Signs at release, including amend replacements. Re-login must re-prepare, not resend the stale signed body. #1785-1788 own `account.rs`. |
| 9 | Polymarket | — | P1, P2, P4, P6 | The raw-credential WS frame becomes `Present` fields. |
| 10 | Paradex | codex-zs5e83 | P2 Stark, P4, P6 | Replace the legacy `cx.sign_request`/`payloadSigner` path with the shared provider while keeping the payloadSigner contract. #1787 owns account/trading files. |
| 11 | Lighter | codex-zs5e83 | P3 Schnorr, P4, P5 | The auth token is minted per use at release. |
| 12 | BreakoutProp | — | P6 | Password `Present` into the login body; the token follows P6. |

Rows 3 and 4 need no prerequisite and can proceed in parallel with ApeX's
shared PR once their RequestContext owners agree on file windows.

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
