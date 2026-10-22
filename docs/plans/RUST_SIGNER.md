# Standard send-time signing

Core plan, base `e42a32a8e945`; owner direction: Sam, 2026-10-22. Signing is a
standard execution boundary, not a remote-signing special case. This PR migrates
the Binance family as the reference; other venues follow in separate lanes.

## Contract

- Venues build unsigned private HTTP requests / WS auth steps. At dispatch,
  their existing sign module prepares canonical bytes using the current engine
  clock and nonce state, requests primitive signatures, and inserts the result.
  Every resend rebuilds time-sensitive fields and signs again. Public I/O bypasses it.
- The engine exposes a host-injected asynchronous `Signer`: algorithm, opaque
  key reference, exact bytes and cheap optional context (venue/method/host/path).
  The signer may refuse; no mandatory allowlists or integrity subsystem in this PR.
  Supported primitive vocabulary covers HMAC-SHA256/512, ed25519, ES256/JWT,
  EIP-712, Stark and Schnorr, and composite operations such as Bitunix's
  SHA256(secret-suffix); exact byte/hash/output contracts and concrete provider
  support are explicit. Unsupported operations refuse before transport.
- Signing secrets stay private to the Signer implementation. Engine, venue
  structs, generic hosts, logs and protocol messages carry references/signatures,
  never signing key material. Raw credential presentation (API-key/passphrase
  headers, WS credentials, password/session bodies) is also a signer operation:
  it fills a named outbound field only at send time. Such values necessarily
  exist in the in-flight request, but engine/venues never retain or log them.
  No venue-specific secret exceptions. The default local in-memory Signer
  owns/clears its keys; hosts receive only its handle. Trusted provider setup
  consumes legacy credentials before venue construction/options serialization.
- Generalize the existing sans-I/O pending-signature action/event pattern.
  Correlate replies with request/attempt/generation; bound pending work and include
  signing in the existing deadline, with an independent timer even if the signer
  never settles. The send boundary owns timestamp/nonce, attempt and generation;
  preparation runs after pacing/queues, and stale completions reprepare within the
  original deadline (including 5 s WS and 120 s JWT windows). Clock input is the
  agreed synchronized `Cx.now` epoch-ms, never a provider wall clock.
  Timeout/refusal before dispatch is `notSent`;
  preserve partial/unknown outcomes if earlier steps already wrote. Late/disposed
  replies never send. A signature receipt is not venue acceptance or idempotency.
- Wasm/JS, native and Workers hosts accept the same injected provider seam and
  default local implementation. Remote/enclave/parent-process providers can use
  it later; this core adds no remote-signing wire or deployed application path.

## Committable phases and proof

1. Audit current eager signing and commit this spec/index first. Add core
   primitive request/result types, dispatch preparation and async park/resume;
   supply an isolated local provider reusing existing crypto, without adding
   crypto dependencies to the engine or heavy WASM dependencies.
2. Move Binance USD-M, PM, COIN-M and Spot HTTP/WS signing through that boundary.
   Keep primary/secondary identity selection, payload spelling, timestamp/nonce
   behavior and retry/outcome contracts. Remove signing secrets from Binance
   venue options/state; adapt trusted construction and tests to provider handles.
3. Wire production/test/native/frame/JS/Workers construction and injected-provider
   regressions. Prove dispatch-time clocks, re-sign on resend, async completion,
   refusal/timeout, late-generation exclusion, no key exposure and unchanged
   command outcome/identity ordering. Keep existing tests/assertions.
4. Add a cheap CI ban: migrated Binance has no credential/crypto escape outside
   its sign step; freeze exact legacy sites in unmigrated families so no new
   violations enter. Each follow-up removes its legacy entries; no claim that
   all venue secrets disappeared before those migrations actually land.
5. Require independent phase/adversarial reviews, focused native/host/SDK checks,
   normal WASM budgets and complete Binance-family replay/request/signature proof
   under tape clocks. Phase 1 local signing must remain byte-identical; no fixture,
   comparator, timing-policy or budget relaxation. CI owns release builds/sweeps.
   Integrate relevant landing, guard separately, merge exact green head and report.

Exclusive core source window; #1707 may land first. After the core merges,
`codex-5c57jl` and `codex-mxb4mq` migrate assigned families in parallel against
its actual API/pin. The server-order memo and Aster profile rollback are held;
no TS deletion/default flip or remote wire work is part of this core PR.


## Reference API and cutover callers

`engine::signing` defines `Signer`, `SignRequest`, `Operation`, `KeyRef` and
closed `SignError` values. `SignStep` supplies venue canonicalization/insertion;
`Cx::sign_http` / `send_signed` stage unsigned actions. `prepare_dispatch(Cx.now)`
and `drain_actions` share one ordered, bounded dispatch service. The default local
provider supports HMAC-SHA256/512, SHA256 and SHA256(secret-suffix); the remaining
primitive vocabulary refuses explicitly until its concrete provider is added.
Local immediate batches preserve replay action/timer ordering. Async batches have
independent deadlines, ordered completion, freshness rechecks and late-reply fences.
Actions emitted by failure callbacks reenter that same dispatch queue; internal
preparation markers never reach a host or compete with a real provider batch.

Primitive results may include optional bounded `headers` alongside values and
receipt. Empty headers serialize identically to the earlier result shape. The
shared validator checks all metadata before applying signatures; only a trusted
HTTP `SignStep::result_header_policy` (default deny), captured by that attempt,
may admit callback headers. Default signing-only captures and credential/WS work
reject them. Declared protected names retain native precedence; other callback
names override case-insensitively after recipe application. Values remain in the
fresh outbound action and share its deadline/revision/cancel/retry retirement;
no transport side map or post-bind mutation is permitted. Current family recipes
remain default-deny until their explicit bridge adoption.

Native callers supply `Engine::set_signer`, `Host::with_signer`, or the trusted
`FrameServer::add_venue_with_signer` registration. Raw JS hosts use
`EngineHost(..., options: { signer })`; managed SDK/Workers hosts also transfer
allocation ownership with `ownsEngine: true`, while raw borrowed callers retain
their existing explicit `free()` responsibility. Disposal drops callbacks and
owned local-provider allocations; freeing after a WASM trap is best effort.
Workers deployments may supply
`createSigner(state, env, venue) -> { signer, identities }`. Only refs enter
`EngineOptions.signing`; the Workers command wire cannot supply provider callbacks.
Unmigrated families reject these refs/providers rather than pretending to use them.

Binance USD-M/PM/COIN-M/Spot and Bybit linear/inverse/spot/demo/HyroTrader
high-level `ExchangeOptions` credential inputs remain
a trusted setup adapter. `RustEngineExchange` extracts them before `BaseExchange`
retains options (incomplete secondary input rejects synchronously with typed
`invalidInput`/`notSent`), caches non-secret private/public construction metadata, and uses
an opaque one-use `LocalSignerSetup`. The raw WASM constructor accepts optional
argument 3, `signerSetupJson`, separately from generic engine-options JSON. It
consumes the setup into a local provider and wipes owned Rust buffers, including
on parse/refusal paths. JavaScript releases its references but cannot promise
physical string zeroization. Custom WASM loaders must forward argument 3 for
all migrated aliases, including Bybit's five variants.

Direct native Bybit callers can no longer put `key`/`secret` or raw secondary
credentials into `safe_cex_venue_bybit::Options`. Use trusted registry `build`
compatibility setup, or construct opaque primary/secondary `Identity` values and
install their provider with `Engine::set_signer`. The direct venue stores only
references; supplying identities without a provider does not enable signing.
High-level SDK key/secret input remains accepted through the trusted setup adapter.

**Required NX adoption:** worker/hosted bootstrap currently copies
`exchange.options`, and signingHost caches keys from it. Provision the worker's
provider through a separate trusted setup channel; generic retained options no
longer carry credentials. Use public/signing-state metadata for old `options.key`
checks. Provider callbacks remain local. Reload/CLI construction from saved account
configuration remains a setup input. Legacy raw native/WASM credential input
normalizes into this same provider path; remove that adapter before TS
venue/reference deletion, after all cutover bootstrap callers migrate. This PR
does not implement remote HTTP/WS signing or claim the pinned NX callers migrated.

The ban in `scripts/check-bans.sh` freezes exact legacy HMAC/ed25519/secret source
sites outside unmigrated sign modules; Binance and Bybit sign modules are scanned
too, with their existing test-only oracle sites frozen. New or duplicated sites fail CI, and
negative controls cover both ordinary venue source and the migrated sign module.
This is a migration guard for those patterns, not proof that every possible
cryptographic primitive or secret spelling is detected. Family migrations remove
their legacy entries as provider recipes replace eager signing.


Clock follow-up contract (not implemented by this core): keep `Cx.now`, staged /
prepared times, deadlines and freshness durations in raw scheduling time. The
clock lane's protocol view supplies only absolute signing stamps. A changed
calibration revision must invalidate an outstanding canonical payload even when
raw time is unchanged, then reprepare within the original raw deadline;
observations of the same offset should remain idempotent.
