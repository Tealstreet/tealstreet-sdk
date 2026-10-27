# Paradex local Stark signing custody

Applies to Paradex SDK, browser/WASM, worker and native callers. Owner:
claude-m26ps4. This is Paradex hardening PR-2; PR-3 moves the remaining
legacy request sites to the managed funnel.

## Construction

The account `key` moves from venue options into trusted signer setup. The
construction rule declares `dispatchSigning`, the `paradex` setup mapping and a
ScalarOnly `starkDigest` primary. The `payloadSigning` bridge is removed, and
private `payloadSigner`/`remoteSigner` accounts still refuse as before
(`unsupported`/`notSent`). NX needs no change: callers keep supplying `key`,
`paradexAddress` and `subkeyAddress`, and the shared construction rules map the
key into the private setup capsule. The capsule is exactly
`{"primary":{"secret":<key text>}}`; caller `localSigner` metadata and secondary
fields refuse. Legacy top-level `secret` and `passphrase` were never read by
Paradex and are consumed and ignored.

A 66-character key is the Stark scalar, parsed exactly as the venue always did:
`0x`/`0X` hex or decimal digits, `0 < k < n`. Any other key is an Ethereum key,
and setup derives the Stark scalar from it on the network's fixed Ethereum
chain (`1` mainnet, `11155111` testnet). Invalid keys now refuse at construction
with `invalidInput` and a fixed message, rather than leaving a public account
with a notice after `/system/config`. Public-only intent ignores the key.

For an Ethereum-derived key, a `/system/config` whose `l1_chain_id` differs
from that network constant refuses private work before `/auth`. It is never
re-derived. The account address is the one the sealed key derives under the
configured class hashes, as before; the supplied address is still only checked
to be nonzero. A direct Stark key keeps the supplied account address and needs
no Ethereum chain or class hashes.

Only trusted local setup provides the key. Engine options carry no key, and a
private account built from injected references, an external, hosted or wrapped
provider, or a transported identity refuses private work.

## Where the sealed key travels

Trusted setup seals the provider's own Stark public key on the engine identity,
bound to that provider's incarnation. The seal is never serialized.

- Native host (`build`, `build_with_signer`): setup and build run in process,
  so the seal travels in process. An external signer refuses private Paradex
  (expected; hosted is post-beta).
- WASM worker: the WasmEngine constructor runs capsule setup and build in the
  worker, so the seal travels in process.
- NX and SDK forwarding: only public options JSON and the private capsule
  cross the boundary. Setup runs on the receiving side, so the seal travels in
  process there.
- exchange-server hosted accounts: `remoteSigner`/`payloadSigner` refuse before
  construction (expected; hosted is post-beta).

## Signing

`/auth` headers and every order, batch leg, TP/SL child and amendment are
signed at dispatch, with the release clock. A batch shares one stamp. Each
provider result must be a canonical 64-byte `r || s` with `0 < r, s < n` that
fully verifies for its own hash under the sealed public key, or no request is
sent. Measured Stark verify cost per signature (starknet-crypto 0.8.1, node
24): 2.43 ms on wasm32 at the shipped `opt-level="z"` (1.44 ms at 3) and
0.38 ms native, under the 3 ms bound, so every signature is verified; a full
10-order batch adds about 24 ms on wasm. Wire bytes are
unchanged at equal clocks for both key forms on both networks.

## Interim legacy request site

Ruled by ddi2il on 2026-10-27: one growth of the request-legacy inventory,
`venues/paradex/src/account.rs` digest
`de9d09faf6d835f5164746112b2915550e9364b218dcf7701191482bbc0b492e`, count 1.
That is the `sign` helper through which `/auth` and every signed write attach
their dispatch-time step. `RULED_GROWTH` in `scripts/check-request-context.py`
admits exactly that tuple and fails as stale once the site is gone. Paradex
PR-3 removes the site and the tuple together. Until then, the trading `hash`
field keeps its name so the 17 existing legacy rows stay byte-identical; PR-3
renames it. `/auth` stays a legacy request with its `direct` route until PR-3.

## Shared engine and crypto API

- `signing::Incarnation` and `Signer::incarnation()`, which defaults to `None`.
  It is a default method, so no host changes. Only `LocalSigner` returns one,
  enforced by `check-credential-only.py`; any wrapper or external provider has
  none, so no seal reads under it. Key and digest correctness does not rest on
  the token: every result is fully verified.
- `Identity::seal_public_key` (trusted local setup only, bans-confined) and
  `Identity::verified_public_key(algorithm, installed)`, plus
  `Cx::verified_public_key(slot, algorithm)`. A seal reads only while its
  signing ref is current and the installed provider presents the incarnation
  that made it. Any `set_signing_ref` call clears it.
- `LocalSigner` draws a fresh incarnation at construction and on every
  material change, and seals each declared Stark signing ref at setup. A
  declared Stark key with no canonical public form now refuses setup.
- `install_scalar_only_primary` admits `StarkDigest` beside `Secp256k1Digest`,
  and `can_sign(StarkDigest)` follows the `stark` feature.

## Removed payload-signing pipeline

Paradex was the only engine consumer of delegated payload signing, so this
change removes `Cx::sign_request`, `PayloadSignRequest`, `SignatureTarget`,
`Action::SignPayload` and `Event::PayloadSigned`/`PayloadSignFailed`, along
with the native `Transport::sign_payload` and
`NetworkTransport::with_payload_signer`, the WASM `payloadSigned` and
`payloadSignFailed` bindings, the JS host `signPayload` op and transport hook,
and the TS Paradex remote-signer transport. The host action/event wire loses the
`signPayload` op. Native embedders that called `with_payload_signer` must drop
it; no in-tree caller remains. The construction record's `bridge.payloadSigning`
field stays on the wire and is now false for every venue.

Post-beta follow-up: Hyperliquid's sealed agent address (engine address setup)
could move onto this carrier. It is already re-derived from the installed
provider after `set_signer`, so this is consolidation, not a fix.
