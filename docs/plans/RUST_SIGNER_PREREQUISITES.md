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

The next phase adds `SignReceipt { stampMs?, expiryMs? }` to `SignRequest` and
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
