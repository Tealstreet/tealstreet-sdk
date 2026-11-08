# Coinbase hosted signing

Both `coinbase` and `coinbasespot` admit RemoteSigner custody and REST-only
PayloadSigner custody. Construction consumes local CDP credentials into an owned
provider; production venue state receives opaque references and public JWT
bindings. Direct `Coinbase::new` remains the existing native compatibility API.
Public-only construction and local setup without a nonempty primary key ignore
both credential slots and start unsigned public discovery, including native and
direct WASM callers. Private primary authority still requires a bound provider.

## Authentication and signed operations

Existing local CDP parsing is preserved: ES256 P-256 PEM (SEC1/PKCS8), Ed25519
base64 seed/keypair, downloaded-key JSON, escaped newlines and bounded input.
The canonical work is `[Present key name, Sign exact JWS input]`. Remote work
contains only `base64url(header).base64url(claims)`, never a private key, seed,
PEM, local credential capsule or opaque reference.

| Operation | Signing boundary |
| --- | --- |
| Private account, permissions, portfolio, balance, positions, orders and history reads | Shared REST dispatch, selected primary/read binding, JWT URI uses method, real host and pathname without query. |
| Ordinary/protective placement, edit, batch cancellation and amend cancellation proof | The same REST dispatch; body and query remain byte-preserved. |
| Private `user` and `heartbeats` subscriptions | One fresh provider JWT per frame, without a URI claim. |
| Public catalogue/ticker/book/candle/trade traffic | Unsigned. |
| DRB foundation CDP authentication | The device codec accepts the existing CDP issuer variant and HTTP `POST drb.coinbase.com/api/v2/public/auth` or URI-free WS JWS. Session acquisition/refresh integration remains owned by the in-flight S2/S3 branches (#3225/#3226); this change does not cut over their private transport. |

JWT nonces use signing entropy, dispatch timestamps use protocol UTC, and tokens
expire at `nbf + 120 seconds`. Captures retain their existing cancellation,
single-use, expiry and clock-revision fences. Device capture rejects clock skew
beyond 30 seconds. The owned provider rejects foreign key names, mismatched
algorithms, noncanonical encodings, extra/duplicate claims and extra operations.
Only a canonical 64-byte ES256 `r || s` or Ed25519 result is published.

The new `coinbase-jws` remote format returns the raw signature as unpadded
base64url. The older `coinbase-jwt` WS request remains a complete fresh JWT API.
RemoteSigner wins when both callbacks exist and never falls back after failure.
PayloadSigner receives `{exchangeName, key, payload, scheme: coinbase-jws,
encoding: base64url, algorithm: es256 | ed25519}` with canonical REST JWS input and returns
`{ signature: <unpadded base64url of 64 bytes> }`; callback headers are refused.
PayloadSigner cannot sign URI-free WS auth, and construction selects REST-only
private feeds. This is delegation of a CDP key's signing operation, not a new
wallet/agent or OAuth credential flow. Those protocols require separate adapters.

SDK Coinbase accounts continue to ignore saved secondary read credentials and
their proxy, with the existing warning; this filtering occurs before provider
setup. Native local callers retain their existing read-key selection and
fallback. Direct hosted-provider setup refuses secondary signing with a typed
`unsupported/notSent` reason. Failed signing is `NotSent`; a released ambiguous
write keeps its existing `Unknown` result and is never resent automatically.
Definite `NotSent` releases that attempt's client-ID/reduction reservation, so
an explicit retry or cancel-all is not blocked by a placement that never left.
An older `Unknown` placement sharing a refused attempt's ID stays fenced.

## Public type changes and NX

- `ExchangeOptions` and saved account config accept optional
  `coinbaseJwtAlgorithm: 'es256' | 'edDsa'`. Hosted origins default to `es256`;
  local devices infer their actual algorithm through the existing Rust parser.
  `accountToExchangeOptions` carries the saved selection for both Coinbase aliases;
  an explicit options override still takes precedence.
- `PayloadSignerRequest` adds the `coinbase-jws` scheme with explicit algorithm
  and base64url encoding. NX payload delegates must implement that scheme before
  using this optional mode.
- `RemoteSignerRequest.wsAuth.outputFormat` and hosted capture input add
  `coinbase-jws`. WASM/EngineHost expose public `coinbaseSigningMetadata()`.
- Both construction rules enable dispatch/remote signing and declare
  `payloadSupported: true`, `payloadRestOnly: true`, protecting `Authorization`.
- NX #9032's refused list follows these declarations automatically. No manual
  list change is needed. NX must forward `coinbaseJwtAlgorithm: 'edDsa'` when
  constructing a hosted origin for an Ed25519 account; default ES256 accounts
  need no new option. Both sides must use the SDK containing this wire codec.

## Verification

The policy regression fails on master. New callback/JWS tests also reproduce
master's missing admission and wrong signer algorithm. Native trading fixtures
compare every dispatched graph/signature with a device capture, including
ordinary/protective writes, cancel/amend, read-key fallback and Unknown paths.
Existing dispatch recipes cover stale/expired results and request/key mismatch.
Actual Coinbase-only WASM tests drive local devices and keyless remote origins,
independently verify Ed25519/P-256 signatures, and cover REST-only payload custody,
malformed/foreign/expired inputs, no callback fallback and no venue I/O.

Protocol references:

- https://docs.cdp.coinbase.com/coinbase-app/authentication-authorization/api-key-authentication
- https://docs.cdp.coinbase.com/coinbase-app/advanced-trade-apis/websocket/websocket-authentication
- https://github.com/coinbase/coinbase-advanced-py/blob/master/coinbase/jwt_generator.py

The implementation preserves existing algorithm compatibility; offline signature
parity does not establish live venue acceptance. No live credentials or orders
are used. WASM caps are unchanged; the lead owns budget-only changes.
