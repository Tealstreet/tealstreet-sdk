# Shared signer setup normalization policy

Plan, 2026-10-22. Fresh base: actual shared provider #1731
`06b321192a65562db61489b2cbcdcfe4fce5d055`. Owner-assigned common core;
no MEXC or other family signer adoption in this PR.

MEXC currently trims its primary API credential but preserves its secondary
credential. The shared trusted SDK/native/WASM setup path has only caller
metadata, so a migration would lose the mandatory family rule. Declare the
per-family/per-slot field policy once and apply it inside trusted setup before
opaque identities are installed. Never trim secrets in a venue or recipe.

1. Audit trusted setup and synchronous public/private intent. Define one
   declarative policy with preservation as the default and MEXC primary-only
   credential trimming. Policy-selected fields are authoritative; caller
   metadata cannot silently undo the venue contract. Keep secret decoding,
   explicit stable IDs and absent/empty passphrases unchanged.
2. Reuse the same policy in native raw compatibility, private WASM setup and
   SDK construction/private-intent derivation, with a single shared source of
   family declarations and generic normalization code. Preserve caller-owned
   options and erase consumed temporary inputs on success/refusal. Verify
   Unicode whitespace semantics and early intent agree across TS/Rust rather
   than introducing independent family switches.
3. RED regressions prove MEXC primary trimming/secondary preservation through
   real provider Present/HMAC results for both legacy and separate setup. Pin
   whitespace-only primary/public intent, both slots, explicit metadata,
   unrelated-family preservation, passphrases/secret bytes/stable IDs and
   malformed input. Keep construction admission for unmigrated families.
4. Run focused native+Binance/registry/host/SDK tests, strict checks and
   independent correctness/adversarial review; use fresh CI and an own-only
   separate guard before landing under the owner queue. Integrate actual
   intervening shared changes normally.

Architecture gate: this extends the shared credential setup choke point.
It is not a per-venue signer or normalization workaround. MEXC adoption still
waits for shared RequestContext actual-release pacing; Phemex separately waits
for raw/protocol expiry projection. Current queue #1734 -> #1735 -> #1728
retains priority; this work may develop locally/CI meanwhile.

Independent plan review resolved the defaults: undeclared fields retain caller
`trimCredential` metadata, rather than forcing preservation. MEXC alone forces
primary trim=true and secondary trim=false over opposite metadata. Trimming
uses existing native Rust Unicode White_Space semantics, with a generic TS
Unicode White_Space predicate for synchronous presence (not JS String.trim);
U+0085 and U+FEFF are explicit parity regressions. SDK primaryComplete and cached
canSign/publicDataOnly use effective presence before credential stripping.

The engine owns non-secret CredentialSetupPolicy/IdentitySetupPolicy data;
construction-rules.json owns family declarations, generated into Rust constants
and consumed by the TS resolver. Crypto applies resolved policy once during
trusted installation. Existing policy-free primitive setup APIs preserve caller
metadata; venue-aware registry raw/JSON adapters are used by official native/WASM
construction. Opaque external providers remain responsible for compliant material.
