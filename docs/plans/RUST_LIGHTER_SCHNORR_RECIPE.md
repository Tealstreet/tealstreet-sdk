# Lighter Schnorr signer recipe

Plan, 2026-10-24. Lane claude-m26ps4; signer owner and reviewer claude-19pt8s.
Orphaned from codex-sy8pn5, which is no longer running. This is P3 of
[RUST_SIGNER_NON_HMAC_MIGRATIONS.md](RUST_SIGNER_NON_HMAC_MIGRATIONS.md) for
Lighter. Part 1 is stacked on #1866 (`0d34535c618`, the Lighter nonce counter).
Public-key derivation (P4) stacks after #1963 lands. Remote recipe ids (P8)
stack after #1926 lands.

## Today

- **Primitive** (`crypto/src/lighter.rs`): ECgFp5 Schnorr over a Goldilocks
  Poseidon2 hash.
  - The key is a 40-byte little-endian scalar.
  - The input is bytes in 8-byte little-endian u64 words.
  - The signature is 80 bytes, `s(40) || e(40)`.
  - The nonce is `hedged_nonce(input, key, entropy[32])`: HMAC-SHA512 keyed
    with the scalar, over a domain tag, the entropy, fp5(input) and a counter.
- **Venue** (`venues/lighter/src/sign.rs`): `Signer` holds the raw key.
  - It is built in `account.rs` `identity_ready`.
  - It signs transactions eagerly in `trading.rs` (`signer.sign(&tx, nonce,
    now, fill_signing_entropy)`) for Create, Cancel and Leverage, sent through
    `sendTx`/`sendTxBatch`.
  - It signs auth tokens (`"{deadline}:{account}:{api_key}"`, zero-padded)
    used in HTTP query strings and in private WS subscribes (`account.rs`,
    `history.rs`).
- **Engine** (`engine/src/signing.rs`): `Algorithm::SchnorrDigest` is
  reserved, with a 64-byte result group. `LocalSigner` returns `Unsupported`.

## Contract (decided here; claude-19pt8s reviews)

`Algorithm::SchnorrDigest` means ECgFp5 Schnorr with Poseidon2 over
Goldilocks, Lighter's scheme.

- **Key:** the signing ref holds a 40-byte little-endian scalar, decoded as hex
  with an optional `0x` (the shared `KeyEncoding::Hex`). It must be canonical
  (below the group order) and nonzero.
  - **Refused, never reduced.** Like `stark_digest`, the arm refuses a
    non-canonical key, a zero key or a key whose public point is zero
    (`InvalidRequest`, never a signature). Legacy reduces non-canonical limbs
    Go-style; this stricter rule is a recorded behaviour change.
  - **Who checks when.** In part 1 the provider arm refuses at sign time.
    Part 3 adds the same refusal at setup for a ref declared
    `signingAlgorithm: schnorrDigest` (#1963). Generic setup has no algorithm
    for a ref before part 3.
- **Payload:** the concatenated payload is `entropy(32) || input`.
  - The provider uses exactly the first 32 bytes as nonce entropy, through
    `lighter::hedged_nonce`, and the rest as the message.
  - `input` is a non-empty multiple of 8 bytes. Each little-endian u64 word is
    reduced mod p exactly as legacy and Go do, so a word at or above p is
    neither refused nor treated differently from legacy. Pinned with
    `go-wasm-noncanonical.json`.
  - The engine draws fresh entropy (`DispatchContext::signing_bytes`) on every
    preparation, including a re-preparation that keeps the #1866 transaction
    nonce.
  - No provider, local, host or remote, may substitute its own randomness.
- **Result:** exactly **80 bytes**, `s || e`. `expected_result_lengths` moves
  SchnorrDigest from the 64-byte group to its own 80-byte entry. The width
  change ships in part 1 together with the arm, so no build exists where a
  provider answers 80 bytes while the engine expects 64. It is a contract
  change for a reserved variant with no callers, recorded in
  RUST_CUTOVER_CALLERS.
- **Provider arm:** `primitives::schnorr_digest(key, payload)` behind a new
  crypto feature, `schnorr`, matching `ed25519`/`es256`/`secp256k1`/`stark`.
  Disabled, it refuses as `Unsupported`, like the other arms. It reuses
  `lighter::hedged_nonce` and `schnorr_sign`; there is no second
  implementation.

## Parts

**Part 1: contract, provider arm and transaction recipe** (on #1866)
- The contract above, the provider arm and the feature.
- A shared canonicalization, `sign::tx_input(tx, nonce, expired_at, ...)`, used
  by both the legacy `Signer` and the recipe. Legacy tests stay green as the
  refactor guard.
- `sign::TxRecipe` (a `SignStep`):
  - `prepare` rebuilds the preimage from the unsigned transaction: it fixes
    `expired_at = stamp + 10 min`, draws 32 entropy bytes, and emits one
    `Sign { SchnorrDigest, identity.signing, [entropy || input] }` with
    `SignReceipt { stamp_ms, expiry_ms: expired_at }`.
  - `apply` checks the 80-byte width and writes `tx_info.Sig` (base64) into
    the `sendTx`/`sendTxBatch` body.
- The transaction nonce comes from #1866's `VenueCounter`. It is fixed per
  request and reused on re-preparation; it advances only on venue 21104
  (P5).
- The venue is not rewired in this part; there is no dual path. Like #1845's
  Aster recipe, the recipe is proven through the real engine dispatcher with a
  stub issuing venue. The admission PR swaps the venue to it in a single
  cutover and deletes the eager path.

**Part 2: auth-token recipe** (on part 1)
- `sign::AuthRecipe` for HTTP `auth=` query strings and the private WS
  subscribe (through `send_signed`), with deadline `stamp/1000 + 60`. The
  canonicalization is shared with the legacy `auth_token`.

**Part 3: public key** (after #1963 lands), per claude-19pt8s:
- `public_key_len` gives `SchnorrDigest => Some(40)`.
- crypto `asymmetric()` includes SchnorrDigest.
- `build_support` and the TS `SigningAlgorithm` union gain `schnorrDigest`.
- `LocalSigner` gets a `PublicKey` arm (`lighter::public_key`), and
  `valid_public_value` a canonical, on-curve ECgFp5 check.

**Part 4: remote recipe ids** (after #1926 and part 3 land)
- `lighter.tx.1` and `lighter.auth.1` in `REMOTE_RECIPES`.
- Per #1963's contract, a family that can verify before release must do so.
  Part 3 gives Lighter a public key, so part 4 adds a shared
  `lighter::verify(public, input, signature)` (Schnorr verify, same hash). It
  is the P8 and host pre-release gate against the bound public key. Venue
  rejection is not the gate.
- If verify falls out of scope, part 4 stays held for claude-ddi2il.

Venue admission (registry identity and custody, the Aster/Backpack pattern) is
a separate later PR. RUST_SIGNER_NON_HMAC_MIGRATIONS row 11 (Lighter, owner
codex-zs5e83) is updated to point here once claude-ddi2il confirms ownership.

## Original REDs (each shown failing on the base first)

- R1 (part 1): `LocalSigner` `SchnorrDigest` sign returns `Unsupported`. The
  test expects the legacy `hedged_sign_with_entropy` signature for the same
  key, input and entropy, with the payload split as entropy (first 32 bytes)
  then input, using the `go-wasm*.json` and `venue-signing-preimages.json`
  vectors. It also covers:
  - non-canonical input words reduced exactly as legacy
    (`go-wasm-noncanonical.json`);
  - non-canonical, zero and zero-point keys refused (`InvalidRequest`).
- R2 (part 1): an 80-byte provider result is refused (`InvalidResult`) by the
  64-byte contract.
- R3 (part 1): dispatch byte-equality. A Create, Cancel or Leverage
  transaction issued by a stub venue through the engine dispatcher and
  `LocalSigner`, with seeded entropy, must equal the legacy `Signer` wire
  `sendTx` body byte for byte. It fails on the base because the provider
  refuses SchnorrDigest (R1) and the 64-byte contract refuses the result (R2).
- R4 (part 1): async. A parked provider (`immediate() = false`) holds the
  transaction until the result arrives. Disposal or a late result releases
  nothing, the transaction nonce is not consumed twice, and a refusal fails
  typed and not sent.
- R5 (part 2): the auth token via `AuthRecipe` equals legacy `auth_token`
  bytes, for both HTTP and WS.
- R6 (part 3): `PublicKey { SchnorrDigest }` returns `InvalidRequest` today.
  The test expects an independent public-key vector, malformed-provider
  refusals through `fetch_public`, and the setup-time key refusal.

## Checker

Once the transaction and auth paths sign through recipes, prune the Lighter
rows in `signing-legacy-sites.json` and add `lighter` to the migrated set in
`check-signing.py`. That happens in the admission PR, when the legacy eager
path is deleted.

Each PR: CARGO_BUILD_JOBS=2 focused tests, an independent review answering
"per-venue fix of a cross-cutting concern?", claude-19pt8s signer-owner review,
and no merge without a window.
