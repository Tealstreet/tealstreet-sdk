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
     signing input through `signing_input`, and emits `Present(credential)`
     plus `Sign(Es256 | Ed25519, signing, input)`;
   - `signing_input` is extracted from `jwt_with_nonce` on this branch as a
     pure refactor (R0 proves the legacy JWT bytes are unchanged). It is the
     same function the P8 branch (#1926) extracts, so whichever lands second
     merges onto an identical body;
   - the receipt declares `stamp_ms` and `expiry_ms = exp * 1000`;
   - at apply it requires the presented key name to equal `key_name`
     (refusing a provider that presents another key), checks the signature
     width, and writes the same two headers as legacy, `Content-Type`
     included on GET;
   - freshness is 5 s, well inside the JWT's 120 s life.
2. **`sign::WsSubscribe::new(identity, key_name, algorithm, channel)`**, a
   `SignStep` for `send_signed` that builds the same subscribe frame with
   a JWT without `uri`. Each channel's frame is its own step. A refusal of
   either reaches the venue's sign-failure hook, which disconnects and
   stops as legacy does today (the conversion wires that hook).
3. **Provider Ed25519 keypairs** (shared, crypto): the Ed25519 arm also
   accepts the 64-byte `seed || public` form Coinbase downloads use. It
   checks that the public half matches the seed and refuses a mismatch,
   instead of a venue truncating the key.
   - Why the provider rather than setup: every local path (native, WASM
     local setup and the hosted browser executor) uses this one Rust
     provider, so local and hosted signing cannot disagree about which keys
     are accepted. Remote JS signers hold their own keys and never receive
     key material from the engine.
   - This is a deliberate tightening. Legacy truncates a keypair without
     checking its public half; the provider refuses a mismatched pair. The
     algorithm doc states the accepted forms, and the conversion records
     the change in RUST_CUTOVER_CALLERS.
4. `JwtAlgorithm { Es256, EdDsa }` becomes public so setup can declare it.

Setup boundary: `Present` compares stored bytes exactly, so trusted setup
must store the trimmed CDP `name` as the credential, the same text the venue
passes as `key_name`.

Out of scope, owned by the conversion or admission: the venue `Options`
identity fields and their consumers (`lib.rs`, `rest.rs`, `account.rs`),
the registry arm, trusted setup that parses CDP JSON or PEM into the
provider and normalizes the `Ed25519:` prefix, construction rules, and the
remote half (P8 #1926 already carries `coinbase.restJwt*.1`).

## REDs (each shown failing before its change)

- R0 (refactor guard, not a RED): the existing legacy JWT tests stay green
  after `signing_input` is extracted, so the bytes are unchanged.
- R1: an Ed25519 keypair (seed || public) is refused by the provider arm.
- R2: a dispatch test cannot sign a Coinbase request through the provider
  (no recipe exists).
- R3: a recipe whose presented credential is another key name must not
  release (apply refusal).

## Tests

- Real engine dispatch with a `LocalSigner` holding an ES256 PKCS8 key and
  an Ed25519 keypair. The released JWT's header and claims match an
  independent literal expectation and the legacy `Credentials` JWT apart
  from the nonce (the engine's entropy draw is not reproducible from a
  venue test), and the signature verifies under the public key (ES256 via
  `p256`, Ed25519 via RFC 8032).
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
