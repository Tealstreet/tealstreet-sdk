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
