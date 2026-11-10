# ApeX Omni hosted signing

This change enables RemoteSigner in construction-rules.json. It does not merge
or release the activation. Private PayloadSigner stays unsupported.

## Authentication and custody

ApeX already used provider-owned dispatch signing. Its missing remote policy
prevented hosted construction, while the shared HMAC adapter could not execute
the contract-dependent signing graph. API authentication uses HMAC-SHA256 with
the base64 text of the API secret. Order placement additionally uses zkLink
contract signatures derived from the existing 65-byte OmniKey seed. The
[ApeX API documentation](https://api-docs.omni.apex.exchange/) describes these
two authorization layers; this SDK exposes contract orders, not fund transfers
or withdrawals, through the delegated zkLink codec.

The signing device retains both signing materials. The origin needs its public
API key and API passphrase for protocol presentation. The latter remains in
provider custody, never generic exchange options or the delegated batch wire.
No Ethereum private key, OmniKey seed, API secret or opaque ref is serialized
into a RemoteSigner request. API-key/OmniKey custody is the delegation supported
by this venue integration; no separate agent or subaccount mode is invented.

## Closed batch wire

The existing RemoteSigner ws-auth transport adds `apexomni-batch`. Its descriptor
contains `contracts` (36-byte contract-order payloads) and `payload` (bounded
literal-byte parts or indices selecting hex contract signatures). Each contract
must occur exactly once in the HMAC graph. The device validates the closed shape,
rejects non-order zkLink transactions, resolves its own primary identity and
builds the same native graph: contract signatures, Present API key, Present
passphrase, HMAC. The response contains only `credential`, `contracts` and
`signature`; the origin injects its own Present-only passphrase into the native
result. Partial, malformed, foreign-account or canceled results refuse atomically.

The trusted HTTP compatibility capture accepts contract-free canonical HMAC
descriptors and returns the required passphrase header locally. HTTP packets
that already contain contract signatures retain their exact serialized body.
Every POST signs the native sorted semantic fields: serialized form and multipart
bodies are decoded for signing, so boundary bytes and transport field order do
not enter the MAC. Malformed bodies and non-text multipart fields refuse.
Actual trading uses the native batch graph and never delegates order-body
construction to the compatibility capture.

| Native request | Signing |
| --- | --- |
| signed symbols; account, balance, open orders; history orders and fills | API-key/passphrase presentation and HMAC |
| ordinary, TP/SL and replacement orders | one or more zkLink contracts, then HMAC over their exact hex signatures |
| delete order, delete open orders, initial-margin-rate setting | API-key/passphrase presentation and HMAC |
| private WS login, including reconnects | HMAC of protocol timestamp plus `GET/ws/accounts`, then the existing stringified login frame |
| public klines | unsigned |

The native one-time signature-rejection retry keeps its frozen L2 order body
and uses its existing legacy HMAC canonicalization. Remote custody changes no
nonce, timestamp, expiry, transport outcome, retry grant or execution fence.

## Admission, public types and callers

The table adds enabled remoteSigning with base64 signature encoding, the ApeX
protected headers and passphrase header. payloadSupported stays false because
generic PayloadSigner cannot execute zkLink contract work. Private PayloadSigner
and all configured secondary signing credentials on private accounts refuse with typed
unsupported/notSent errors. RemoteSigner takes precedence and never falls back.

Explicit `publicDataOnly: true` is resolved before private signer validation.
These accounts, including those with no key, ignore unused callbacks,
passphrases and setup metadata, install no provider or identities, and cannot
sign. Captured public mode remains public even if later construction input
supplies a key or requests private mode.

Public type changes are the additive `apexomni-batch` RemoteSigner/host input
format and the Apex HMAC/zkLink capability declaration. Hosts/devices must run
the SDK containing the device codec. NX #9032 derives its refused list from
construction-rules, so no handwritten NX refusal-list change is needed.

## Deterministic verification

Tests were first run red for missing admission/capabilities and the unsupported
native device format. Native fixtures compare the actual device and local
operation graphs and resulting signature bytes. Existing independent npm SDK
1.0.2 zkLink vectors remain the crypto control. TypeScript tests cover batch
ownership, raw-wire redaction, provider errors, cancellation, unsupported modes
and construction rules. Actual WASM tests exercise local/device and keyless
origin custody, full HTTP packets, WS preimages and dependent contract/HMAC work
with network access forbidden.
Shared POST goldens are checked by both the native signer and the HTTP capture
for leverage, ordinary/protective orders, cancel and cancel-all; batch placements
and cancellations keep the same per-packet canonicalization.

The venue README owns protocol restrictions. Scoped checks and any build
limitations are recorded in the PR/report; deterministic tapes are not live
account connectivity evidence. WASM caps are left for the lead's budget-only work.
