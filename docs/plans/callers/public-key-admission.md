# Public-key admission helpers

These helpers reuse the existing signing providers and native HTTP transport.
They add no account registry, signer storage, engine, socket envelope, or venue
activation. The join owner retains its existing feature gates, pacing, connection
generation, lease checks and admission deadline. Both hosted signing modes use
the same helpers.

Enable the `safe-cex-crypto/admission` Cargo feature to use the shared statement
and verifier APIs. The native server requests this feature explicitly; existing
venue WASM builds leave it disabled. Signer-family features remain independent.

`safe_cex_crypto::admission::Statement` is a closed descriptor with a fixed kind,
an attempt, an identity digest, a 32-byte nonce and an issue time. The server
creates it with `LocalProof::issue`; devices reconstruct the signing request with
`Statement::sign_request`, using their captured account's opaque signing ref.
No URL, request body, transaction or caller-supplied digest is part of the codec.
`Statement::sign` is the synchronous adapter for an existing local provider.
The join owner binds the descriptor to the device account before dispatch.

| Family | Existing algorithm | Admission message |
| --- | --- | --- |
| Ethereum wallets | `Secp256k1Digest` | EIP-712 `Admission(bytes32 challenge)` under the `Tealstreet Admission` v1 domain |
| Ed25519 | `Ed25519` | Fixed admission prefix followed by the challenge digest |
| Stark | `StarkDigest` | Domain-separated challenge hash restricted to 250 bits |
| Lighter | `SchnorrDigest` | Fixed word-aligned admission prefix followed by the challenge digest; the existing provider adds its hedged nonce |

The Ethereum domain and primary type differ from venue typed transactions.
The Ed25519 bytes do not use the venue request grammar. The Stark input derives
only from the closed admission descriptor. Lighter's first word is outside the
venue transaction chain-ID range. Existing venue descriptor parsers and signing
algorithms retain their behavior.

`LocalProof::verify` accepts a signature only for its original connection, before
the existing signing deadline expires on either clock, and consumes the attempt
on success or refusal. `verify_candidate` uses that same consumption check and
derives the candidate public value through the existing provider's public-key
operation. It never exports or compares secret material and does not mutate a
retained provider. The caller retains ownership of temporary provider memory.
Neither method attaches an account or replaces final session/lease checks.

`Family::for_venue` classifies unambiguous public-key families. AsterDEX v3 uses
`Family::Ethereum` after the caller selects its existing v3 credential policy;
the venue name alone cannot select it because v1 uses a different algorithm.

For Lighter, `fetch_lighter_registered_key` uses the account's existing transport
and a captured authority guard. It requests one account/index from a fixed
network origin, bounds response size and duration, and returns only a canonical
registered public key matching that exact account/index. The returned bytes must
be included in the canonical identity before issuing its statement. Dropping
the read closes its native event channel; no request is retried or cached here.
The endpoint and response fields follow the official
[Account API](https://github.com/elliottech/lighter-python/blob/main/docs/AccountApi.md#apikeys)
and [API-key model](https://github.com/elliottech/lighter-python/blob/main/docs/ApiKey.md).

These are composable primitives for the shared join path. They do not change the
currently enabled venue set or add a new device wire handler. A local check
establishes possession; ordinary engine startup reads retain venue-validity
handling.

## Closed join-proof contract

The additive v1 payload uses the existing `sign-req` and `sign-res` envelopes:

```json
{"t":"sign-req","d":{"id":"request-1","type":"join-proof","exchangeName":"backpack","accountName":"account","instanceId":"instance-1","joinProof":{"version":1,"statement":{"kind":"tealstreet-admission-v1","attempt":"attempt-1","identityDigest":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"nonce":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"issuedAtMs":1000}}}}
```

Each array contains exactly 32 integer bytes. A success is
`{"t":"sign-res","d":{"id":"request-1","joinProof":{"version":1,"signature":"<canonical padded base64>"}}}`.
A refusal is `{"t":"sign-res","d":{"id":"request-1","error":"Join proof refused"}}`
or the fixed product string `Join proof unavailable`. Arbitrary callback error
text is not a wire refusal. The response contains no public key, statement,
account configuration, URL, transaction or client assertion of validity.

`admission::wire::Request::from_json` and `Response::from_json` are the closed
Rust payload decoders. They bound JSON to 4096 bytes and reject duplicate,
unknown, missing, null, mixed legacy signing fields and unsupported versions.
Discriminants are JSON strings only; Serde externally tagged unit-enum objects
are refused, including the nested Statement kind and public-facts venue/network.
Statement decoding retains its 2048-byte bound; `issuedAtMs` must fit an unsigned
JavaScript safe integer. Correlation labels are nonempty, untrimmed,
control-free UTF-8: id/instance <=128 bytes, account <=512 bytes. Signatures use
64 bytes for Ed25519/Stark, 65 for recoverable Ethereum, 80 for Schnorr. Shape
validation is not verification: the pending owner must still correlate id,
check its original owner/socket/instance/attempt and call `LocalProof::verify`.
No response field supplies that authority.

The readonly public `JoinProofRequest`, `JoinProofResponse`, `JoinProofStatement`
and `JoinProofEnvelope` types describe this contract. They are separate from the
legacy `RemoteSignerRequest` dispatch until the corresponding device/server
handlers exist. The contract itself enables no device handler, capability or venue readiness. Existing HTTP and WS-auth consumers retain their
closed legacy inputs. Unknown join-proof capabilities must never waive proof or
select a different signing mode.

## Canonical public facts

`admission::identity::Projection::from_json(verified_owner, facts)` is a pure,
bounded projection, not an account registry or evidence of key provenance. The
owner comes from captured authenticated authority. Facts come from the captured
credential policy and authoritative public metadata, never a proof reply. Missing
facts refuse; do not infer the signer from a balance-query address or substitute
a client-echoed registered key. Lighter metadata and Paradex account/key
association remain the caller's responsibility before projection.

Facts contain exactly `venue`, `network` (`mainnet` or `testnet`) and the tagged
`credential` below. All fields are required; nested objects reject unknown fields.
Public bytes use `0x` plus fixed-width hex (32-byte Ed25519/Stark/account,
20-byte Ethereum address, 40-byte Schnorr public key). Hex digits canonicalize to
lowercase; keys use the existing public-value validator. Backpack/Orderly callers
must decode the venue's API-key representation to these public bytes first.

| Venue | Credential kind and fields |
| --- | --- |
| Backpack | `ed25519`: `publicKey` |
| Hyperliquid/base+spot | `agent`: actual `signer`, `accountScope: {kind: "main"}` or `{kind: "vault", address}` |
| GRVT | `grvt`: `tradingAccount` (opaque, case retained), actual `signer` |
| TxFlow | `wallet`: `signer` |
| Polymarket | `polymarket`: `signer`, `funder` |
| AsterDEX v3 | `aster-v3`: `user`, `signer`; name alone cannot select this policy |
| Orderly, KodiakFi, Mode, What, WOOFi | `orderly`: `account`, `publicKey` |
| Lighter/base+Robinhood | `schnorr`: `accountIndex` (u64 decimal string), `apiKeyIndex` (u8), `registeredPublicKey` |
| Paradex | `stark`: `account` (canonical field element), `publicKey` |

The scope is compact JSON `[venueNamespace, network, ""]`. Hyperliquid's base
and spot views use `hyperliquid`; every other exact venue keeps its own namespace,
including Orderly aliases and `lighter-robinhood`. No supported local-proof
family has a regional-site selector. The key id is a compact JSON array: kind,
then the table's fields in order, except the agent uses signer, scope kind,
optional vault address. Lighter indices canonicalize to unsigned decimal strings;
other opaque fields retain their exact bytes. The frozen fixture records every
array explicitly.

The identity digest reuses `SHA256("tealstreet/identity/v1" || len64be(owner) ||
owner || len64be(scope) || scope || len64be(keyId) || keyId)`. Lengths count UTF-8
bytes. Labels, secrets and proof replies are absent. This helper does not change
the existing server registry rollout predicate or resident keying.

`crypto/tests/fixtures/join-proof.json` freezes public-only vectors produced with
Python's SHA-256/JSON implementation independently of this projector. The existing
crypto admission test target checks all projections and challenge digests;
`node --test sdk-rs/harness/join-proof-contract.test.mjs` independently checks the
binary hashes and strict TypeScript contract. Existing per-family admission tests
continue to verify the domain-separated messages with the existing providers.

## Pending local verification

The native server has a shared pending local-proof strategy. It uses the existing
isolated SignerLink, one-active/four-waiting identity queue, start pacer and final
registry attachment. Canonical public facts
are captured by the server before queueing; a proof reply never supplies the
expected key or a verification decision. Exact Paradex selects this strategy from
its captured public account address and subkey public key, preserving native
decimal/hex field spellings. The public init key must match the mode's existing
account/subkey layout. Invalid facts refuse without selecting private startup.
The existing device dispatcher signs the closed Stark recipe; DELEGATED verifies
its temporary protected provider through the same strategy. Venue authorization
remains ordinary startup's responsibility. Other venue activations must supply
their own authoritative facts and complete device handler.

## Device challenge dispatch

`RustEngineExchange.signJoinProof(requestJson)` forwards the original JSON string
to `EngineHost.hostedJoinProof` and WASM `prepareJoinProof`. It never parses and
re-serializes the request: the frozen Rust decoder rejects duplicate, mixed and
missing fields before preparing work. The captured engine venue and owned primary
signing reference select the existing admission-only family recipe; caller JSON
cannot select a key or supply transaction bytes. AsterDEX requires explicit V3
construction policy; HMAC venues have no public-key proof recipe.

Proofs and ordinary hosted signing share one capture pool, provider runner,
deadline and disposal path. Captured purpose metadata selects the closed output,
which contains only the original request ID and signature. A proof expires within
the original statement's 15-second budget and the existing five-second signing
capture limit. Completion is single-use; malformed output, cancellation, expiry
and replacement retire the capture. There is no HTTP or socket fallback.

The SDK retains its original worker owner/generation, initialization promise and
signing host across deferred work. An installed unbound owner refuses; callers
without the optional owner hook retain the existing same-owner behavior. The NX
transport must additionally capture the original account, registration, instance
and socket before calling this entry and recheck them before publishing a reply.
This handler performs no Session admission and enables no server venue. Existing
constructor/custody and compiled-feature restrictions still apply; a generic
method's presence is not proof of venue readiness.

The reusable `hosted-join-proof` WASM feature is linked by default only in the
Backpack candidate family (and therefore the combined build). Other standalone
families retain their existing size-critical composition. Their activation slices
must explicitly opt into the feature and certify their own optimized size before
shipping. This build selection does not change the server admission catalog.

## Pending verification lifetime

After queue admission, a fresh random server attempt and nonce bind the closed
statement to that captured identity. COSIGNED replies retain their raw JSON until
`Response::from_json` has checked duplicates and mixed fields. The local verifier
receives the actual authenticated socket that delivered the reply. A pending
local proof cannot migrate or replay on signing-link suspend/attach. Ordinary
admitted signing keeps its existing resume behavior.

DELEGATED verifies the incoming temporary protected provider through its opaque
primary reference. It does not borrow a retained resident key or ask the device
to sign. Failed proofs never fall back to private startup or change a resident.
A verified permit retains its original socket and instance; the Session completion
and registry attachment recheck captured user, generation, token, lease and the
remaining admission deadline before holding or publishing the account. Queueing
and pacing do not restart the local admission budget. No proof result is cached.
