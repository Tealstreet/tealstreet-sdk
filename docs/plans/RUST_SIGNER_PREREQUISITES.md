# Shared signer migration prerequisites

Plan, base `ad0e6fa1e42` (#1714); owner decisions: Sam/claude-ddi2il,
2026-10-22. Complete these shared contracts before family signing migrations.
Binance remains the reference; no additional venue migration in this PR.

## Contracts

1. **Credential setup and identities.** Extend trusted `LocalSetup` and opaque
   engine identities with passphrase references. Credential bytes stay within
   the provider; `Present` materializes a value only into outbound I/O. Accept
   an explicit, non-secret stable account identity from trusted setup, separate
   from credential/key references. Never derive it from key material. Legacy
   Binance inputs may omit it; identity/nonce-dependent migrations must require
   it rather than invent a fingerprint. MEXC should prefer a verified venue uid,
   otherwise this stable id; persisted old MEXC ids need cutover mapping. Kraken
   nonce scoping uses the same opaque setup id. Reused credentials across engines
   or slots need a consistently assigned id; account consumers use the intended
   account mapping. A slot name or key reference is never an identity fallback.
2. **Trusted key decoding.** Decode explicit UTF-8/base64/hex/PEM formats inside
   the local provider, with bounded input, strict validation, fixed errors and
   zeroizing owned buffers. Preserve UTF-8 defaults and existing Binance bytes.
   Credential normalization is setup policy, separate from the stable id.
   Add no heavy dependencies or unsupported algorithm claims.
3. **Signed WS upgrades.** Expose a public `Cx` attachment hook for an unsigned
   WS-connect action. It uses the existing dispatch queue, deadlines, socket
   revocation, freshness and async completion; no second connection executor.
4. **Signing entropy.** Give `SignStep` a dispatch context containing the engine
   clock and access to the injected signing entropy from #1624. Identity replay
   seeds never replace signing entropy. Recipes draw at preparation/repreparation;
   no ambient venue randomness or retained nonce cache. Provider errors remain
   closed typed pre-dispatch failures.
5. **Signing receipts.** Canonical preparation declares the actual protocol stamp
   and optional expiry used, expressed as protocol UTC milliseconds. The Signer
   returns that receipt with its result;
   the engine validates it against the current attempt, then notifies the venue's
   pending request only when fresh outbound work is released. No provider clock,
   deadline extension or stale/failed receipt publication. Phemex expiry repair
   and Bitrue previous-send timestamps will consume this shared fact. Update
   native/WASM/JS/Workers provider types together, with explicit caller impact.

Session tokens (listenKey, KuCoin bullet URLs, Kraken Spot tokens) are transient
credentials, not long-lived signing secrets. Their lifetime/refresh moves into
one later engine WS-auth boundary. Families migrate HTTP signing first and track
one session-token follow-up each; no new venue caches meanwhile. Mandatory request
context/outcome sealing and protocol-clock calibration remain separate follow-ons.

## Phases and proof

- Extend setup/ref/decoder contracts and focused provider regressions first.
  Preserve trusted legacy construction, sanitized generic options and one-use
  WASM setup. Require explicit stable ids only where the consuming policy needs
  them; do not silently change the Binance caller contract.
- Add dispatch context and WS-upgrade staging with nonce-stream isolation,
  reprepare/failure, hung-provider timeout, late completion, close and disposal
  controls. Keep raw scheduling durations and the agreed clock-domain seam.
- Add receipts across provider/engine/host boundaries, including wrong/missing
  receipt rejection, no receipt after timeout/staleness, and exactly-once pending
  metadata update for the final successful attempt. Keep action ordering intact.
- Independent phase/adversarial review must answer the architecture gate:
  **shared engine/provider contracts, not per-venue policy copies**. Retain every
  incoming test/assertion; list any intentional API/test shape changes in the PR.
- Run focused Rust/host/SDK checks locally (`CARGO_BUILD_JOBS=4`); release WASM,
  size/timing gates and complete nine-case/90-output replay proof run in CI.
  Preserve native exact-signature assertions and existing parity-policy limits;
  normalized archive equality alone does not prove original wire signatures.
  Integrate relevant landing, review, green CI, separate fresh guard, merge and
  report the actual pin before migration lanes begin.

## Setup API

`LocalSigner::from_setup(LocalSetup)` now returns `Result`; `from_json` and trusted
registry adapters retain closed constructor errors. `LocalIdentity` accepts optional
`passphrase` and `stableId`, `secretEncoding` (`utf8` by default; `base64`, `hex`,
`pem` explicit), and `trimCredential` (false by default). Encoding is not inferred.
PEM accepts PKCS8/SEC1 envelopes and decodes their bytes; it does not add a new
signing algorithm or bypass that algorithm's key validation. Encoded fields are
bounded to 1 MiB and setup JSON to 4 MiB; malformed input never echoes material.

`Identity` carries only `credential` / `signing` refs, optional `passphrase` ref
and optional non-secret `stableId`. `require_stable_id()` rejects absent/invalid
mapping; it never substitutes a key hash or slot name. Callers assign consistent
ids across engines/slots sharing a nonce domain and map account identities
explicitly. Existing Binance input can omit this metadata.

SDK `ExchangeOptions.localSigner.{primary,secondary}` carries the three metadata
fields; ordinary key/secret/passphrase fields remain trusted input. The one-use
capsule consumes both before generic options are retained and forwards them via
WASM constructor argument 3. Unmigrated Rust families reject the metadata with
`unsupported` / `notSent`. Worker/hosted callers still need a separate trusted
bootstrap channel, as required by #1714; this does not implement remote signing.

## Dispatch and receipt API

`SignStep::prepare` takes `&DispatchContext` instead of a bare clock value.
`now_ms` is the calibrated protocol view; `signing_bytes` draws only from the
engine's independent signing source. Raw staging/deadline/freshness durations and
clock-revision checks remain in dispatch. `Cx::connect_signed` preserves ordinary
socket metadata/suppression and stages the upgrade through that same queue.

The provider contract adds `SignReceipt { stampMs?, expiryMs? }` to `SignRequest` and
structured provider results `{ values, receipt }`. Both fields are independently
optional: expiry-only and credential-presentation recipes must not invent a signed
timestamp. Present values must be finite, nonnegative safe integer protocol UTC
milliseconds; expiry must follow stamp when both exist. The provider echoes the
exact current attempt's declared record. Missing, different or invalid records
fail before materialization. Dispatch checks expiry as well as freshness and clock
revision again at release, without extending its raw deadline.

One no-`Cx` venue callback receives an HTTP pending value or socket identity and
validated receipt only after final fresh release. It can update pending metadata,
not emit recursive I/O. This proves engine release/materialization only, never
transport send or venue acceptance. Native/WASM/Workers and custom JS providers
must return the structured result together; no old-array fallback is planned.

Already-expired preparation fails before invoking a provider, avoiding a synchronous
stale/reprepare loop with a local signer at an unchanged clock. Binance declares
its actual signed stamp only: `recvWindow` stays a duration under the existing
freshness policy, including legacy zero/fractional inputs; it is not relabelled
as an absolute expiry. Real expiry recipes declare their own exact normalized
UTC-millisecond value. Receipts do not change signed bytes.

SDK caller impact: explicit custom Rust `Signer` implementations and JS/Workers
`EngineHost` callbacks must return the structured record and echo the declared
receipt. The default local provider adapts internally; ordinary SDK users do not
need a new callback. `SigningTarget::Http` exposes the mutable venue pending value
without its engine timer/deadline; `SigningTarget::Socket` names the released
authentication action. No callback has `Cx` or credential bytes. Incoming clock,
scheduler and financial regressions remain intact.

## Non-HMAC schema extension (ApeX P0/P1/P3)

Plan, 2026-10-23, base `fa5d5fb90c4`. Owner: claude-19pt8s. Schema sign-off
was given by codex-uufub4 (placement) and claude-m26ps4 (nonce and
credential semantics). It is the first shared prerequisite of
[RUST_SIGNER_NON_HMAC_MIGRATIONS.md](RUST_SIGNER_NON_HMAC_MIGRATIONS.md).
This PR enables no family.

- **Extra signing refs (P0).**
  - Names come from the closed enum `ExtraKey`, initially `zklinkSeed`.
    Polymarket's `l1Wallet` is added with its family.
  - `Identity.extra` maps `ExtraKey` to `KeyRef`. The closed enum bounds
    count and names; each ref keeps the existing `KeyRef` bounds.
  - Extra refs are signing material only. They are never a nonce domain, an
    account subject, or a stable-id, account or slot fallback.
  - Extra refs travel inside the selected slot's identity.
- **Setup policy.**
  - A family declares its extra key in `IdentitySetupPolicy.extra` as
    `{ key, field, encoding }`.
    - `field` is `optional` or `requiredForPrivate`. There is no fallback
      variant, because extras never cross slots.
    - A declared `encoding` is authoritative over caller metadata.
  - The local provider binds each extra ref to its own algorithm. A
    `zklinkSeed` ref is refused for `Present`, credential interpolation and
    every other algorithm, and `ZkLinkContract` refuses ordinary refs.
  - Trusted setup rejects duplicate JSON names and any undeclared extra.
  - Required extras follow the passphrase rules for complete slots.
- **Key encodings (P1).**
  - `KeyEncoding` moves to the engine signing schema; crypto re-exports it.
    This is a direct cutover.
  - It gains `base64Text`: provider key bytes are the ASCII of standard
    padded base64 of the input bytes. ApeX's HMAC key is the first user.
  - A family may declare `IdentitySetupPolicy.secretEncoding`. Like trim, a
    declared value is authoritative over caller metadata.
- **`ZkLinkContract` (P3).**
  - Key: exactly 65 decoded seed bytes. Payload: exactly 36 contract bytes.
    Result: raw 64-byte packed signature.
  - The local provider reuses `safe_cex_crypto::zklink` behind the crypto
    feature `zklink`, which only ApeX enables. Builds without ApeX therefore
    do not link franklin-crypto, and the provider returns `Unsupported`.
  - No entropy and no nonce state: the zkLink nonce stays client-id-derived
    protocol data.
- **Mirrors.** These change together:
  - WASM `host.d.mts`: `SignAlgorithm`, `SigningIdentity.extra`, and the
    `secretEncoding` options;
  - SDK `construction.ts` and `localSignerSetup.ts` policy and metadata
    types;
  - the registry `build_support.rs` policy generator.
- **Not yet wired:** SDK `LocalSignerSetup` and the delegated identities do
  not yet carry `extra`. The ApeX adoption PR adds that capsule input with
  its family declaration.
- **Proof.**
  - ZkLink: independent npm-1.0.2 vectors already pinned for ApeX, plus
    exact-length refusals.
  - Base64Text: an independent Node vector.
  - Duplicate and undeclared extra refusals.
  - A redaction check that no extra material appears in refs or errors.
  - Feature-off `Unsupported`. CI also runs the crypto signer tests without
    workspace feature unification.
  - Focused engine, crypto, registry and SDK checks run locally; CI owns
    WASM size and timing.

Implementation checkpoint: the engine, crypto, registry generator, WASM types
and SDK types are implemented as above, plus the derived zkLink seed wipe.
Focused checks pass locally:
- engine lib (542 tests);
- crypto signer without and with `zklink` (16 and 44 tests);
- registry (71 tests);
- Binance, Bybit and ApeX (801 tests);
- SDK signer setup tests, typecheck and lint;
- strict workspace Clippy, fmt and bans.

Independent adversarial review: CLEAN. Its minor findings are addressed:
policy-owned seed encoding, the narrowed field vocabulary, the CI feature-off
step, the seed wipe and role binding. CI owns the WASM size and timing gates.

## Public derivations (P4)

Plan, 2026-10-24. Owner claude-19pt8s. Approved by claude-ddi2il as its own
engine PR off landing. Reviewers: codex-uufub4 (engine) and codex-hcmq3f
(consumer). It unblocks the Hyperliquid, TxFlow and GRVT recipes and P6b
(Polymarket `l1Wallet`). The Paradex Ethereum-to-Stark derivation stays out
until Sam's B/C decision.

**Problem.** Venues still derive public values from key material: the
Hyperliquid and TxFlow agent addresses (`account.rs:315`, `lib.rs:1077`,
TxFlow `lib.rs:51`), the GRVT signer address in the order body, and the
Polymarket address. Once a venue holds only refs it cannot do that, and it
must never receive the key to try.

**Operations** (engine `signing.rs`; non-secret results, no payload):
- `Operation::PublicAddress { key }`: the 20-byte Ethereum address of a
  secp256k1 key.
- `Operation::PublicKey { algorithm, key }`: the public key for that
  algorithm's key form. `Ed25519` gives 32 bytes, `Es256` 65-byte
  uncompressed SEC1, `StarkDigest` the 32-byte big-endian felt, and
  `Secp256k1Digest` 33-byte compressed SEC1. Any other algorithm is
  `InvalidRequest`. Lighter's key is added with its own algorithm.
- Both obey role binding: an extra ref answers only for its role's algorithm
  (and `PublicAddress` only for a secp256k1 role, P6b's `l1Wallet`).
- Validation:
  - result widths are exact;
  - a request mixing public operations with `Sign`/`Present`/`Sha256` is
    allowed, since public values are never secrets;
  - `remote` (P8) still requires exactly one `Sign`, so a public operation
    is never sent as a recipe op.

**Local provider.** It implements both from installed material, through the
existing primitives (`ethereum::private_key_address`, `ed25519::public_key`,
`starknet::public_key`, `p256` public key), behind the same crypto feature
arms. Results are public bytes and never echo key material.

**Setup-time fetch.**
- Local providers answer synchronously (`immediate()`).
  `safe_cex_venues::fetch_public(provider, ops) -> Result<Vec<Vec<u8>>,
  SignError>` runs a public-only request through `sign_ready` inside trusted
  construction. It returns `Unavailable` for a non-immediate provider instead
  of blocking.
- The family's admission maps results into non-secret `Options` fields such
  as `agent_address` or `signer_address`, the same way public config already
  arrives. The venue reads config; it never derives.
- Host and remote providers cannot answer synchronously at construction.
  This needs a reviewer decision:
  - **A.** Their public values are declared account config, supplied with
    the account (the browser or remote signer knows its own address). Setup
    fails closed when a required value is missing, and a startup check
    verifies it after the first signature where a family can (recovery,
    P8 verifiers). No engine seam is needed.
  - **B.** An engine startup seam `Cx::fetch_public(ops, pending)` emits
    `Action::Sign` with public-only work, and `Venue::on_public` receives
    the bytes before private startup.

  A is smaller and matches how keyless accounts already carry public
  identifiers; B is only needed if a family must start before any
  configured value exists. I recommend A.
- JS host contract: `SignOperation` gains `publicAddress` and `publicKey`
  kinds in `host.d.mts`. `remoteSignerProvider` answers them only from
  declared public identity metadata and refuses otherwise; it never forwards
  them to the remote signer.

**REDs** (each shown failing first):
- R1: a `PublicAddress` request for a secp256k1 key is refused today (no
  operation).
- R2: an extra ref answers a public operation outside its role.
- R3: `fetch_public` on a local provider returns the address equal to the
  legacy `private_key_address` for the same key, and Hyperliquid's legacy
  derivation for a corpus key.

**Tests.**
- Widths and refusals per algorithm.
- The local provider matches independent vectors: ethers addresses, RFC 8032
  public keys, the starknet.js public key and p256 SEC1.
- Role binding, a mixed request, and P8 `remote` refusing public operations.
- `fetch_public` returns `Unavailable` for a non-immediate provider.
- TS provider: `publicAddress` answered from metadata, refused without it.

**Architecture gate.** This is the shared choke point: one provider
operation for every family that needs a public value, with no venue
derivation.
