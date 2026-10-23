# Managed HTTP unsigned header declarations

Fresh branch from actual #1765 `a3a89719d4079c97d56297bb4738fdc13c659ceb`.
Owner assigned this shared prerequisite ahead of the BingX signer migration,
with WEEX next. RequestContext still owns admission, deadlines, preparation,
binding, physical release, cancellation and response evidence.

## Contract

Add `SignStep::unsigned_http_headers(&self) -> &'static [(&'static str,
&'static str)]`, defaulting to an empty declaration. The declaration is trusted,
static, noncredential codec data. Credentials, passphrases and signatures stay
in provider/apply-time results. It grants no callback-header authority.

`queue_managed` reads the declaration exactly once, after existing read-only
context/mode/capacity checks and before allocation, registration, timers or
Prepare. Shared header validation checks borrowed declarations against the
existing token/control grammar and fixed count/name/value/aggregate bounds
before allocating header strings. Invalid input returns the original pending
value and typed `Invalid`, without consuming IDs or producing actions.

Valid declarations populate the retained unsigned `HttpRequest.headers` in
their exact declared order; `signer_headers` remains empty. Route reprepare and
signer refresh reuse that snapshot rather than call the declaration again.
Default-empty recipes, plain managed HTTP, legacy HTTP and WS keep their paths.

## Scope and first consumer

The engine trait, shared validator and managed queue are the only policy seam.
Extract BingX's existing public SOURCE/Content-Type values into one static
method-specific declaration used by its legacy renderer. Preserve signed legacy
order SOURCE, APIKEY, Content-Type and every URL/body byte. APIKEY is excluded
from the declaration. WEEX later reuses the engine projection with its own
native public header declaration, preserving its credential-first wire order.

A BingX integration test uses this exact declaration with an explicitly
synthetic managed recipe to prove preparation and signing attestation. It does
not claim that BingX production managed requests or provider construction are
admitted by this prerequisite. Its actual first production SignStep consumer
lands in the separate BingX migration, which must shrink the legacy-request
inventory and retain its pending-owner/publication fences. Do not peel a dormant
full recipe into this shared change merely to claim production adoption.

## Gates

1. Review this API and scope. Add the additive default-only trait surface plus
   original assertion-RED preparation tests before wiring the queue; retain
   default-empty compatibility controls.
2. Implement bounded borrowed validation and one-time projection. Test invalid
   names, control bytes, count/value/aggregate overflow, original pending return,
   no IDs/actions/provider calls on refusal, and retained snapshot reuse through
   route/sign preparation. Keep all existing HTTP lifecycle and signing fences.
3. Extract BingX static declarations and prove exact independent GET/POST legacy
   header vectors plus the synthetic managed preparation/attestation controls.
   Do not add any legacy request site or inventory exemption.
4. Run focused engine/BingX/Binance tests, strict Clippy/fmt/bans and independent
   correctness/adversarial/preservation reviews. Include a same-PR caller section.
   Draft PR, full CI and fresh replay/package proof use current landing inputs;
   observe the #1775 landing hold and normal separate guard/merge rules.

Architecture gate: this extends one engine managed-HTTP ingress choke point.
Venue constants describe native public bytes; they do not implement admission,
signing custody, header permissions or release policy.

## Local checkpoint evidence

Both independent plan/API and source reviews are CLEAR. The default-only API
and regression checkpoint `5306944be89` reproduces two assertion failures on the
unchanged queue, with the empty-declaration control passing. Projection corrects
both without changing those assertions. Seven engine controls cover ordering,
route snapshot reuse, invalid final pairs, exact UTF-8 byte/count/aggregate bounds,
original pending/no-allocation refusal, early context/mode refusal and full
managed capacity. The latter uses an existing valid read Context because
creating a 257th Context reaches that separate registry's bound first; the
earlier fixture failure remains retained and is not a production failure claim.

Two BingX tests pin independent native header vectors across GET/POST/PUT/DELETE
and use the explicitly synthetic managed recipe with the shared declaration.
Focused engine/BingX/Binance all-target tests pass 1,027 tests; strict Clippy and
fmt and bans pass. Full CI, authenticated replay/package proof and actual family managed
adoption remain pending; this records no size, performance or runtime-cutover
claim.
