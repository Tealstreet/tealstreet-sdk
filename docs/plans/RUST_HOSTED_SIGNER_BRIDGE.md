# Hosted signing through the owned provider

Plan, 2026-10-22. Fresh base: actual #1735
`b1dfdb43291d5a6de7dd8808b486adddda4cb118`. Separate shared prerequisite
before Bybit #1728; no dependency on its unlanded source or setup-policy #1738.

The public RustEngineExchange.signWsAuth callback still falls back to
BaseExchange, which reads options.secret. Dispatch-signing construction consumes
that material into the trusted provider, so the existing positive Bybit hosted
fixture fails. Native send_signed WS authentication does not prove the separate
hosted browser/server callback works. Retain and repair that positive contract.

1. Inventory all public sign* APIs, current remote/hosted callsites and exact
   input/output behavior. Include signHttpRequest and signWsAuth; distinguish
   pre-existing unsupported HTTP cases from behavior removed by migration.
   Share any additional missing shared canonicalization contract before adapting it.
2. Route migrated-family hosted signing through the SAME owned injected provider
   used by dispatch. Engine::signer() already supplies an opaque provider handle;
   add the narrow AnyEngine/WASM/JS host surface, using owned identity references
   rather than hard-coded names or caller-supplied signing-key references. Build
   bounded primitive SignRequest operations from the hosted semantic request;
   never expose a generic secret-export operation or restore raw engine options.
3. Reuse local sign_ready and the host's existing asynchronous signer callback,
   SignRequest/SignResult validation and sanitized failures. Preserve exact
   input bytes/output encoding and request IDs, cancellation, bounded timeout,
   disposal/late-completion fencing and zero network startup for signing alone.
   No new crypto implementation, venue credential storage, remote signing wire
   protocol or engine dispatch/request-lifecycle policy is introduced.
4. RustEngineExchange lazily creates its existing host and selects this path for
   migrated families. Unmigrated families keep their current legacy/kept-TS path.
   Inventory HTTP support explicitly and reuse actual recipe/canonicalization
   contracts for any supported public API; never return fabricated signatures.
5. Preserve the positive hosted fixture and add independent end-to-end provider
   tests for every migrated family/alias, including local trusted setup and
   async injected/keyless providers. Assert no retained raw secrets and no HTTP
   or WS I/O, exact independent signatures, malformed/error/timeout/disposal
   behavior and unchanged unmigrated compatibility. Bybit integration must run
   the same positive flow after its actual migration is enabled.
6. Run focused native engine/Binance/registry/host/SDK and strict gates, independent
   correctness/adversarial review, rebuilt-WASM CI and authenticated replay proof.
   Keep RequestContext host wiring coordinated with codex-uufub4; no engine.rs
   dispatch edits planned. Fresh own-only separate guard precedes merge, then
   #1728 real-integrates this actual pin and refreshes CI/proof; #1739 follows.

Architecture gate: shared hosted compatibility is a cross-cutting signing
concern, solved at one host/provider choke point. Venue recipes may describe
canonical bytes; no venue-local crypto executor or retained-secret fallback.
Source implementation starts only after the inventory/concrete API audit.
