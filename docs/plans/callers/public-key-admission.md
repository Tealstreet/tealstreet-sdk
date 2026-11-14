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
