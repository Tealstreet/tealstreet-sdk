# Coinbase signer recipe

Plan, 2026-10-24. Owner claude-19pt8s (signer). Row 7 of the family table in
RUST_SIGNER_NON_HMAC_MIGRATIONS.md; picked up from the orphaned non-HMAC lane
with claude-ddi2il's approval. Consumer: codex-hcmq3f's Coinbase conversion
(RUST_COINBASE_REQUEST_CONTEXT.md, #1881). Stacked on #1844 for the provider's
ES256 and Ed25519 arms.

## Today

- `sign.rs` builds the JWS in the venue: `Credentials` holds the parsed
  ES256 key or Ed25519 seed, and `jwt_with_nonce` builds
  `base64url(header).base64url(claims)` with `alg`, `typ`, `kid` and the
  nonce, plus `iss`/`aud` per key kind, `nbf`, `exp = nbf + 120`, `sub = kid`
  and an optional `uri`.
- REST signs eagerly at venue send (`rest.rs:97`), using the read key for
  account and history GETs and the trading key otherwise. The header pair is
  `Authorization: Bearer <jwt>` plus `Content-Type: application/json`.
- The private socket subscribes `user` and `heartbeats` with one JWT each
  and no `uri` (`account.rs:1031`).
- The nonce comes from venue signing entropy (`uuid_with_entropy`).

## Target

The venue holds only `Identity` refs plus non-secret config: the key name
and the JWT algorithm. Signing happens at release through the provider.

1. **`sign::Http::new(identity, key_name, algorithm, method, path)`**, a
   `SignStep`:
   - the outbound URL must be exactly `https://api.coinbase.com` + `path`;
   - at prepare it draws the nonce from `DispatchContext::signing_bytes`
     (same 21-byte uuid form), takes `nbf` from `stamp_ms`, builds the exact
     signing input through the existing `signing_input` (shared with the
     legacy signer and the P8 recipe), and emits `Present(credential)` plus
     `Sign(Es256 | Ed25519, signing, input)`;
   - the receipt declares `stamp_ms` and `expiry_ms = exp * 1000`;
   - at apply it requires the presented key name to equal `key_name`
     (refusing a provider that presents another key), checks the signature
     width, and writes the same two headers as legacy.
2. **`sign::WsSubscribe::new(identity, key_name, algorithm, channel)`**, a
   `SignStep` for `send_signed` that builds the same subscribe frame with
   a JWT without `uri`.
3. **Provider Ed25519 keypairs** (shared, crypto): the Ed25519 arm also
   accepts the 64-byte `seed || public` form Coinbase downloads use. It
   checks that the public half matches the seed and refuses a mismatch,
   instead of a venue truncating the key.
4. `JwtAlgorithm { Es256, EdDsa }` becomes public so setup can declare it.

Out of scope, owned by the conversion or admission: the venue `Options`
identity fields and their consumers (`lib.rs`, `rest.rs`, `account.rs`),
the registry arm, trusted setup that parses CDP JSON or PEM into the
provider and normalizes the `Ed25519:` prefix, construction rules, and the
remote half (P8 #1926 already carries `coinbase.restJwt*.1`).

## REDs (each shown failing before its change)

- R1: an Ed25519 keypair (seed || public) is refused by the provider arm.
- R2: a dispatch test cannot sign a Coinbase request through the provider
  (no recipe exists).
- R3: a recipe whose presented credential is another key name must not
  release (apply refusal).

## Tests

- Real engine dispatch with a `LocalSigner` holding an ES256 PKCS8 key and
  an Ed25519 seed. The released `Authorization` JWT is byte-equal to the
  legacy `Credentials` JWT built with the same entropy draw, and verifies
  under the public key (ES256 via `p256`, Ed25519 via RFC 8032).
- GET with a query (the `uri` claim keeps only the pathname), POST, and the
  WS subscribe frame for both channels.
- Refusals before any HTTP: a different host or path, a key-name mismatch
  and a wrong algorithm.
- Provider keypair tests: a valid keypair signs the same as its seed, and a
  mismatched public half is refused.

## Architecture gate

Is this a per-venue fix of a cross-cutting concern? No. The JWT canonical
bytes stay in the venue's `sign.rs` and reuse one `signing_input`. Signing
goes through the shared `SignStep` and provider. The keypair form is added
once in the shared Ed25519 arm, not per venue.
