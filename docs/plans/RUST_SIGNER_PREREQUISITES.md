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

## Local non-HMAC primitives (P2)

Plan and implementation, 2026-10-23, stacked on the schema extension above.
The local provider implements the declared `Ed25519`, `Es256`,
`Secp256k1Digest` and `StarkDigest` contracts. Each arm sits behind its own
crypto feature: `ed25519`, `es256`, `secp256k1` and `stark`.

- **Gating.** The venue-facing primitive modules stay available. A family
  enables its arm only when it signs through the provider, so other WASM
  builds do not link that code. A disabled arm returns `Unsupported`.
- **Exact contracts.**
  - `Ed25519`: 32-byte seed, exact message, 64-byte signature.
  - `Es256`: PKCS8 or SEC1 P-256 DER from trusted PEM decoding; one SHA-256
    of the message; fixed r || s.
  - `Secp256k1Digest`: 32-byte key and exact 32-byte digest;
    r || s || recovery 0/1 (not ethers' 27/28).
  - `StarkDigest`: canonical 32-byte big-endian felts for key and digest
    (values at or above the modulus and a zero key refuse); r || s.
- **Proof.** Independent vectors: Node Ed25519, RFC 6979 A.2.5 P-256 "sample",
  pinned-ethers recoverable signatures and starknet.js signatures. Each arm
  has width refusals, and the disabled-arm `Unsupported` test runs in CI with
  the features on and off.

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
  - Local provider (`LocalSigner`, which role-binds only extra refs):
    - an ordinary installed ref is `Supported` for `HmacSha256`,
      `HmacSha512` and `Sha256SecretSuffix`, and, once #1844 lands, for
      its arms (ed25519, ES256, secp256k1, Stark), which accept ordinary
      refs (before that they are `Unsupported`);
    - an extra ref is `Supported` only for its role's algorithm, when that
      crypto feature is compiled in (`ZkLinkContract` needs `zklink`);
    - every other combination, and any ref not installed, is `Unsupported`.
  - JS host provider: installing it clears the Rust provider, and the host
    fulfils `Action::Sign`. host.mjs takes `signerAlgorithms` beside `signer`
    and passes it as JSON to WASM `useExternalSigner(algorithmsJson?)`, which
    parses exact `SignAlgorithm` names into `Engine::set_host_algorithms`
    before startup. Undeclared means `Unknown`. The list is per
    algorithm, not per ref. WASM and native local setups install the Rust
    `LocalSigner` directly, so they need no list and local Amend keeps
    working on every host.
  - `remoteSignerProvider` declares `hmacSha256` only
    (`REMOTE_SIGNER_ALGORITHMS`, the same constant its refusal check uses),
    until P8 adds recipe ids. `DelegatedSignerSetup` passes it as
    `signerAlgorithms`. It also refuses secondary-slot HMAC at sign time, which a
    per-algorithm list cannot express. That is acceptable because the answer
    is advisory and release still refuses before I/O.
  - Browser `hosted_signing.rs` is a request facade for the TS exchanges, not
    an engine `Signer`, so it is out of scope here.
- **Engine-held identities.** Trusted construction registers the slot to
  `Identity` map with the engine, next to `set_signer`
  (`set_identities(signing::Identities)`, before startup, refs only). The venue
  keeps its own copy for signing; the registry passes the same values.
  `Engine::identities()` exposes them (refs only) for host tests.
- `Cx::can_sign(slot: CredentialSlot, needs: &[Need]) -> Result<(), SignError>`,
  where `Need` is `(KeyUse, Algorithm)` and `KeyUse` is `Signing` or
  `Extra(ExtraKey)`. It is callable from `on_command` and any command or
  preflight admission point, before the first release.
  - The engine resolves the identity from its registered map. An
    unregistered slot, or a `Need` whose ref the identity lacks, is
    `SignError::InvalidRequest`.
  - Every `Need` must be `Supported`. `Unsupported` or `Unknown` gives
    `SignError::Unsupported`. Both errors map to `NotSent`
    (`signing.rs:539-549`), so the venue reports a whole-command typed
    `NotSent` before any I/O.
  - Refs are resolved for every need before any capability is asked, so a
    malformed command is `InvalidRequest` regardless of need order.
  - No Rust provider means the host fulfils `Action::Sign`: the declared list
    answers, and an undeclared host is `Unknown`.
- There is no `DispatchContext::can_sign`: at prepare the step's own
  `SignRequest` already refuses before I/O, so a second query adds nothing.
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
- Engine: local `Supported`/`Unsupported` per ordinary ref, extra role and
  feature; an unregistered slot or missing ref is `InvalidRequest`; `Unknown` default fails closed; a host with no
  declaration is `Unknown`, and a later undeclared install clears an earlier one.
- Host: a host.mjs provider with and without a declared list; a
  `remoteSignerProvider` refusing `zkLinkContract` and accepting
  `hmacSha256`.
- Consumer proof (hcmq3f's PR): a keyless remote ApeX Amend is a typed
  `NotSent` with zero HTTP actions; local Amend still sends the cancel and
  the replacement.

**P8 interaction.** When P8 admits recipe ids, `remoteSignerProvider` answers
`Supported` for an algorithm only through a matching admitted recipe id.

## Public derivations (P4)

Plan, 2026-10-24. Owner claude-19pt8s. Approved by claude-ddi2il as its own
engine PR off landing. Reviewers: codex-uufub4 (engine) and codex-hcmq3f
(consumer). It unblocks the Hyperliquid, TxFlow and GRVT recipes and P6b
(Polymarket `l1Wallet`). The Paradex Ethereum-to-Stark derivation stays out
until Sam's B/C decision.

**Dependencies.** Key-use tagging (`KeyUse`/`Need`, #1899) and the
non-HMAC `Sign` arms (#1844). Before those land, `LocalSigner` refuses every
non-HMAC `Sign`, and P4 stacks on them.

**Problem.** Venues still touch key material for public values:
- Hyperliquid `account.rs:315` warns when the key does not match the
  configured API wallet.
- Hyperliquid `lib.rs:1077` deactivates spot when key and wallet do not pair.
- TxFlow `lib.rs:51` checks that the key parses.
- GRVT `lib.rs:725` warns on a mismatch, and `trading.rs:784` deliberately
  signs with the derived address so stale config cannot block trading.
- Polymarket needs the `l1Wallet` address.

These are mostly checks, not derivations. P4 moves the derivation behind the
provider and keeps every check and its outcome. A venue holding only refs
must never receive the key.

**Operations** (engine `signing.rs`; non-secret results, no payload):
- `Operation::PublicAddress { key }`: the 20-byte Ethereum address of a
  `Secp256k1Digest` key.
- `Operation::PublicKey { algorithm, key }`. Only `Ed25519` (32 bytes) and
  `StarkDigest` (32-byte big-endian felt) for now. `Es256` and compressed
  `Secp256k1Digest` have no consumer and are deferred. Any other algorithm
  is `InvalidRequest`.
- **Key typing.** A public operation is answered only for a ref that setup
  policy declares as an asymmetric key of exactly that algorithm.
  The declaration is `IdentitySetupPolicy.signing_algorithm` (trusted family
  policy in `construction-rules.json`, never caller setup); #1899's `KeyUse`
  only selects the ref. An HMAC or undeclared ref, a credential, a passphrase
  or an extra-role ref is refused. Otherwise the result would be a stable
  fingerprint of a secret. Each slot's ref answers only for its own
  declaration. There is no request-level expected-slot check, so two
  separately declared slots may both be fetched. Replacing a ref's material
  (`insert`) drops its declaration. Role refs declare nothing yet, and
  `zklinkSeed` answers nothing. `l1Wallet` derivation (an EIP-191/EIP-712
  `Secp256k1Digest` role) is held for P6b's own slice, which adds the role and
  its declaration.
- **Key forms.** Each algorithm reads the same installed form as its `Sign`
  arm:
  - `Secp256k1Digest`: raw 32 bytes;
  - `Ed25519`: 32-byte seed or the checked 64-byte keypair (#1948);
  - `StarkDigest`: the felt below the curve order.
  
  The engine checks exact widths. `fetch_public`, the trusted setup boundary,
  also checks every provider's answer semantically through the crypto crate:
  a canonical, nonzero, on-curve Stark x and a canonically encoded
  (y < p), decodable, non-weak Ed25519 point. The engine stays crypto-free.
- **Mixed requests.** Public operations may share a request with
  `Sign`/`Present`/`Sha256`. Ordering, atomicity and dependencies are
  unchanged. P8 `remote` still requires exactly one `Sign`: a remote request
  carrying a public operation is refused before any work. The existing
  `bind` `_ =>` arm already does this, and a test pins it.

**Local provider.** Answers from installed material through the existing
primitives, behind the same crypto feature arms. Results never echo key
material.

**One channel: declared, bound account config (option A; both reviewers
agree).** There is no engine startup seam (`Cx::fetch_public` is dropped).
- **Binding.** A public value is account metadata bound to the trusted
  identity, slot, exact designated `KeyRef`, algorithm or closed role, and
  generation.
- **Distinct roles.** The TxFlow agent address is not the main wallet,
  application or vault, and the Polymarket `l1Wallet` address is not the
  imported L2 API identity, funder or maker. Each binds only to its own key
  role.
- **Missing or conflicting metadata refuses setup**, before any private I/O. There is no fallback to
  another ref or slot.
- **What a declared value is.** Trusted account configuration. It is not a
  cryptographically verified derivation, and never a new subject or nonce
  selector.
- **Local provider.** Trusted setup calls
  `safe_cex_venues::fetch_public(provider, ops)`:
  - public-only, bounded by op count, and validated before any provider
    check;
  - runs through `sign_ready`;
  - returns `Unavailable` without scheduling for a non-immediate provider.
- **Two values, never merged.** The *expected* value is venue config:
  GRVT `signer_address`, the HL `key`/`user` wallet. It can be stale. The
  *actual* signer value is the one fetched (local) or bound to the `KeyRef`
  (host/remote) above. Only the actual value is signed with or checked by
  recovery. The expected value only drives the native notices and the
  deactivate. `applicationId`, the vault or the main wallet never stands in
  for signer metadata.
- **Admission compares, never overwrites.** When a value is declared,
  admission compares it with the fetched one, and a mismatch keeps today's
  outcome:
  - the Hyperliquid and GRVT notices;
  - Hyperliquid deactivating spot;
  - TxFlow refusing a key that does not parse.

  The expected value is never replaced or filled by the actual one. Each
  check runs at start against the actual value: fetched for a local
  provider, the bound metadata for host/remote. Both comparisons below are
  case-insensitive. A local fetch failure maps to the family's
  "underivable key" outcome today, never a setup refusal.
  - HL `account_start` compares the actual value with `key.or(user)` and
    emits Warn/`CONFIG_INVALID`, then continues. A fetch failure gives the
    same warning (today's `map_or(true)`).
  - HL standalone private spot emits Error/`AUTH_REJECTED`, then deactivates
    and returns before `spot_meta`. It does so when the signing ref or the
    declared `key` is absent, when the fetch fails, or on a mismatch.
    Spot never fills `key`.
  - GRVT warns (Warn/`CONFIG_INVALID`, interpolating the actual address) only
    when the secret ref and `signer_address` exist and the fetch succeeds.
    A fetch failure is silent at start, and the order fails at
    `trading.rs:784` as today. Orders sign `Signature.s` with the actual
    address (`trading.rs:784/987`), so a stale `signer_address` warns and
    continues.
  - Host/remote: the start checks use bound metadata, so HL spot still
    deactivates at start on a mismatch. Missing metadata never reaches
    start, because setup refuses it first. Per-dispatch
    recovery is a second gate on the actual signature, not a replacement.
- **Host and remote providers.** The value must be declared. Where the
  family's signature is recoverable (EIP-712/191 for Hyperliquid, TxFlow,
  GRVT and Polymarket), the family recovers the signer and compares it with
  the declared value *before* physical release, import or any private
  effect, in `SignStep::apply` or the P8 verifier, through the one shared
  `safe_cex_crypto::ethereum::signed_by` (low-s only, v 0/1 or 27/28). The engine has no crypto
  dependency, so the check cannot sit in dispatch itself. It compares only with the
  actual signer metadata, never the expected config. An actual-signer
  mismatch refuses with zero HTTP; this is never a post-send startup check. The comparison is
  against the declared *signer* only: the Polymarket SignerType3 contract
  maker or funder is never compared as if it were the recovered wallet. A
  family that cannot verify before release
  stays held for an explicit owner decision rather than trusting the
  declaration.
- **P6b consumes this channel.** The `l1Wallet` address comes from
  `fetch_public` (local) or bound metadata (host/remote). P6b has no
  separate setup-metadata path and no run-time `PublicAddress` fallback.
  P4 does not by itself make P6b or keyless import ready.

**JS.** No new `SignOperation` kinds: under A nothing at run time asks a
host for a public value. Host and remote metadata is keyed by ref in the
account setup policy (`localSignerSetup.ts`/`construction.ts` types), and
setup refuses when it is missing.

**REDs** (assertions on existing APIs, shown failing first; tests of new
APIs are labelled as such, not REDs):
- R1: Hyperliquid construction from refs only (no key in `Options`) cannot
  run its agent-wallet check today; it reports a missing key instead of
  comparing.
- R2: through the existing dispatch, a host-provider EIP-712 signature
  recovering to an address other than the declared one is still released.
- R3: P6b's `l1Wallet` address cannot be produced without the key in the
  venue.

**Tests.**
- New API: widths, canonical, on-curve and field-bound refusals per
  algorithm.
- Independent vectors: ethers addresses, RFC 8032 public keys, starknet.js
  public keys.
- Refusals for an HMAC/untyped ref, credential, passphrase, cross-slot ref,
  and an out-of-role extra ref.
- Mixed request ordering. A P8 remote request with a public operation
  refuses before work.
- `fetch_public` returns `Unavailable` without scheduling for a
  non-immediate provider, and over-bound requests are refused.
- Mismatch notices and the spot deactivate still fire, through the
  Hyperliquid and GRVT admission.
- GRVT, local and host/remote: a stale expected `signer_address` warns and
  orders still sign and send with the actual address.
- Local fetch failure: HL perps warns, HL spot deactivates, GRVT stays
  silent at start and fails the order. Nothing refuses setup.
- HL with no `key`: perps compares with `user()`, and spot deactivates.
- HL: the agent, main wallet and vault stay distinct. This extends the
  existing `spot_local_signer_identity_is_separate_from_main_wallet_and_vault`
  control to refs and host metadata.
- Missing or conflicting host metadata refuses setup.
- A recovered-signer mismatch refuses before release.

**Delivery split.** The engine PR (this branch) delivers:
- the operations and their validation;
- the `signing_algorithm` declaration in policy, the generator and TS types;
- the local provider arms;
- `fetch_public`;
- `signed_by`.

Its REDs are the existing-decode assertions: setup policy and `SignRequest`
decode refuse the new field and kinds today. The engine-level tests above
cover the widths, refusals, ordering and `fetch_public`. The per-family items
come with each family's conversion, which also writes its own venue REDs (the
R1-R3 shapes above):
- admission mappings;
- HL/GRVT/TxFlow outcome tests;
- pre-release recovery wiring.

The P8 remote-refusal test lands with #1926, where `bind` lives. The Ed25519
keypair form arrives with #1948. P6b's `l1Wallet` role derivation is P6b's
slice. This source does not prove the per-family pre-release enforcement or
the compare-not-overwrite admission; each family conversion gates on those.

**Architecture gate.** One shared provider operation and one setup channel
for every family that needs a public value, with no venue derivation. The
pre-release recovery check is one shared helper that every family calls,
not a check written per venue.

## Asymmetric key typing (P4 follow-up)

Plan, 2026-10-24. Owner claude-19pt8s; requested by claude-ddi2il. Reviewers:
codex-uufub4 (engine/custody) and claude-m26ps4 (credential semantics). The
gap was found reviewing Lighter #1967. It is cross-family, so it is fixed
once in the provider.

**Problem.** `LocalSigner` signs with an asymmetric algorithm (`Ed25519`,
`Es256`, `Secp256k1Digest`, `StarkDigest`, `SchnorrDigest`) using any
ordinary ref. A MAC secret that happens to parse as a valid key (a 32-byte
HMAC secret is a valid secp256k1 or Ed25519 key) therefore signs. The
signature is then a stable function of the MAC secret under another
algorithm. In the other direction, a wallet key can be used as an HMAC key.
P4 closed this for public operations through `signing_algorithm`; `Sign`
still has the gap.

**Rule** (provider; `can_sign` mirrors it):
- Asymmetric `Sign` on an ordinary ref requires
  `signing_algorithm == algorithm`, declared by trusted setup policy.
- MAC `Sign` (`HmacSha256`, `HmacSha512`, `Sha256SecretSuffix`) on a ref
  declared asymmetric is refused, so a declared wallet key is never a MAC key.
- Undeclared refs keep MAC behaviour exactly, so HMAC families are untouched.
- Role refs are unchanged. A role is its declaration (P6b), and
  `ZkLinkContract` stays role-only.
- **Check order.** The declaration check runs before the algorithm arm. An
  undeclared ref is `InvalidRequest` even in a build without the arm, and a
  declared ref in such a build stays `Unsupported`. Tests that pin
  `Unsupported` therefore use declared setups.
- Refusals are `InvalidRequest` and happen before any signing. Injected
  host/remote providers enforce their own custody; this is the local
  provider's rule.

**Declarations for families that sign asymmetric today** (tip `6a6845c4491`):
- **AsterDEX V3** signs `Secp256k1Digest`; V1 is HMAC on the same slot. The
  declaration therefore depends on the resolved API version.
  - The registry's `signer_setup_policy_for(venue, &EngineOptions)` refines the
    static policy for setup-mapped families. For Aster V3 it declares
    `secp256k1Digest` on both slots: the primary wallet, and the read agent's
    own wallet or the primary alias. V1 declares nothing.
  - Both trusted paths use it: native `prepare_local_signer_for` and WASM
    `local_signer_from_json_for_mode`, which gains the parsed options it
    already has.
  - **Version source.** It reads only the explicit resolved
    `asterdex.api_version` that the venue uses: native setup resolves it in
    `asterdex_setup` before the policy is built, and TS sends it explicitly.
    It never re-infers the version from the shape of the material. `None`
    declares nothing, which fails closed because the venue already refuses a
    missing or contradictory version.
  - **Alias.** A V3 read agent with `signingFromPrimary` has no ref of its
    own. It signs through the primary's ref and its single declaration: no
    second copy, and nothing silently dropped. A V3 read agent with its own
    hex wallet gets its own declaration.
  - The TS mapping and golden are unchanged; the declaration is Rust policy,
    never caller setup.
- **Backpack**: `backpack_provider` installs its seed with a policy
  declaring `ed25519` (it used policy-free `from_setup`).
- **Coinbase** (recipes only, not admitted): `Es256` or `Ed25519` by key type.
  Its test setup declares the algorithm, and the conversion (#1881) declares
  it from the key format in trusted setup.
- **Unlanded, ordered after combo-5b**: Orderly #1937 (`ed25519`), Lighter
  #1967 (`schnorrDigest`; its part 3 already plans the declaration) and P6b
  #1973 (roles, unaffected). This PR updates their test setups once they
  land, which is why it touches `crypto/src/signer.rs` after them.

**Raw `insert`.** Tests that `insert` asymmetric material get a trusted
`LocalSigner::insert_declared(ref, bytes, algorithm)`.
- It accepts only asymmetric algorithms.
- It is atomic: it replaces any old declaration in the same step.
- It refuses a role ref, and an algorithm that differs from the one setup
  declared for a ref an aliased secondary shares. That binding survives a
  plain `insert`, which keeps a shared ref's algorithm the way a role keeps
  its role, and only `remove` ends it. Unshared raw refs may be retyped.

Plain `insert` on an unshared ref stays undeclared (P4); on a shared alias
ref it keeps the slots' algorithm (above).

**Version parity.** The refinement reads the same explicit
`asterdex.api_version` field the venue reads, so setup and the venue never
disagree about V1 and V3. Native setup resolves it, defaults and overrides
included, before the policy is built. With `None`, both refuse.

**REDs** (existing API, shown failing first). Each one is its own test per
algorithm or case, so every variant runs. In the default build every RED
fails. With all arms enabled, `es256` and `starkDigest` already refuse the
32-byte test secret through key-form checks, so the undeclared case there
proves the rule only through `secp256k1Digest`, `ed25519` and
`schnorrDigest`.
- R1: a 32-byte HMAC secret installed undeclared signs `Secp256k1Digest` and
  `Ed25519` today; it must be `InvalidRequest`.
- R2: a ref declared `secp256k1Digest` signs `HmacSha256` today; it must be
  `InvalidRequest`.
- R3: `can_sign` reports `Supported` for undeclared asymmetric use wherever
  the arm is implemented. Today the arms are not implemented, so this is
  pinned as a control, not a RED.
- Controls: Aster V3 setup (native and JSON) signs `Secp256k1Digest`, and V1
  still signs HMAC. Backpack, Coinbase and existing HMAC families are
  byte-unchanged.

**Tests.**
- Every refusal pair: undeclared, wrong declaration, MAC on a declared ref.
- The alias rule.
- Aster V1/V3 on both setup paths, including:
  - the V3 Code (EIP-712) endpoints signing `Secp256k1Digest` on the declared
    ref;
  - the aliased read agent and a read agent with its own wallet;
  - `api_version: None` declaring nothing.
- `registry/tests/backpack_admission.rs`, kept in the focused runs. Its valid
  RFC 8032 seed case would expose a missing declaration that the setup probe
  masks as a width error.
- The Coinbase recipes.
- `insert_declared` refusing a MAC algorithm.
- Disabled-arm `Unsupported` with declared setups.

**Ordering.** This touches `crypto/src/signer.rs`, which #1937, #1967 and
#1973 also change. It is ordered after combo-5b and merges their landing
before review.

**Architecture gate.** Is this a per-venue fix of a cross-cutting concern?
No. It adds one provider rule for every algorithm family, and declarations
come from shared trusted policy. The only family-specific code is Aster's
version refinement, inside the shared policy function.
