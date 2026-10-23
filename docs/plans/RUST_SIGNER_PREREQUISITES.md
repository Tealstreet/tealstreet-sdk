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

## Signer capability (command admission, ahead of P8)

Owner claude-19pt8s; consumer codex-hcmq3f (ApeX Amend and batch). Approved
by claude-ddi2il on 2026-10-23 for the engine-window queue, ready-first.

**Problem.** A command can need several signing algorithms across several
releases. Native ApeX Amend releases an HMAC cancel, then a zkLink
replacement. With a remote provider that accepts HMAC but refuses
`ZkLinkContract`, refusal at the replacement's `prepare` comes after the
cancel has left. The result is an effect, not a whole-command `NotSent`.
Nothing trusted tells the venue in advance what the provider can sign for a
given identity:
- ref text, extra presence, raw options and `Signer::immediate()` are not
  capability, and venue flags are banned;
- `capabilities.rs` is venue-level.

**API.**
- `Signer::can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability`
  where `Capability` is `Supported | Unsupported | Unknown`. The default is
  `Unknown`. Providers answer from what trusted construction installed and
  never sign to probe.
  - Local provider: `Supported` only when the ref is installed, role-bound
    to that algorithm (#1839), and the crypto feature is compiled in.
    Otherwise `Unsupported`.
  - JS host provider (host.mjs): a declared `algorithms` list, validated at
    admission like the rest of the host contract. Undeclared means `Unknown`.
  - `remoteSignerProvider`: `hmacSha256` only, until P8 adds recipe ids.
    Every other algorithm is `Unsupported`.
  - Browser hosted signing: per family, from `hosted_signing.rs`.
- `Cx::can_sign(slot: CredentialSlot, identity: &Identity, needs: &[Need])`
  `-> Result<(), SignError>`, where `Need` is `(KeyUse, Algorithm)` and
  `KeyUse` is `Signing` or `Extra(ExtraKey)`. It is callable from
  `on_command` and any command or preflight admission point, before the first
  release. `DispatchContext::can_sign` gives the same answer at prepare.
  - The slot must match the identity the venue selected for it (C1
    `Slots::select` once landed). A mismatch, or a missing ref for a `Need`,
    is `SignError::Invalid`.
  - Every `Need` must be `Supported`. `Unsupported` or `Unknown` gives
    `SignError::Unsupported`, which the venue maps to a whole-command typed
    `NotSent` before any I/O.
  - No provider: `Unavailable`.
- The answer is advisory for admission only. Signing at release stays the
  choke point, so a provider that answered `Supported` and later refuses
  still fails that release as today.

**Use.** A multi-release command (ApeX Amend/batch with contracts) calls
`cx.can_sign` once with every algorithm it will need, before releasing
anything. Single-release commands need no call; `prepare` refusal already
fails before I/O. There is no eager signing and no venue flag.

**Architecture gate.** This is the cross-cutting fix. Any family with a
multi-algorithm command uses the same query: ApeX now, later Paradex (Stark
plus JWT) and Polymarket (EIP-712 plus L2 HMAC).

**Tests.**
- Engine: local `Supported`/`Unsupported` per role and feature; slot and
  identity mismatch is `Invalid`; `Unknown` default fails closed; no provider
  is `Unavailable`.
- Host: a host.mjs provider with and without a declared list; a
  `remoteSignerProvider` refusing `zkLinkContract` and accepting
  `hmacSha256`.
- Consumer proof (hcmq3f's PR): a keyless remote ApeX Amend is a typed
  `NotSent` with zero HTTP actions; local Amend still sends the cancel and
  the replacement.

**P8 interaction.** When P8 admits recipe ids, `remoteSignerProvider` answers
`Supported` for an algorithm only through a matching admitted recipe id.
