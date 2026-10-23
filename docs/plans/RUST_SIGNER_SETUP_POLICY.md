# Shared signer setup field policy

Plan, 2026-10-22. Fresh base: actual shared provider #1731
`06b321192a65562db61489b2cbcdcfe4fce5d055`. Owner-assigned common core;
no MEXC or other family signer adoption in this PR.

MEXC currently trims its primary API credential but preserves its secondary
credential. The shared trusted SDK/native/WASM setup path has only caller
metadata, so a migration would lose the mandatory family rule. Declare the
per-family/per-slot field policy once and apply it inside trusted setup before
opaque identities are installed. Never trim secrets in a venue or recipe.

1. Audit trusted setup and synchronous public/private intent. Define one
   declarative policy preserving caller metadata by default and applying MEXC primary-only
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

Signing-key trim (2026-10-23):
- `trimSigning` is the per-slot companion of `trimCredential`. It trims the
  signing-key text once at trusted setup, before decoding, with the same
  Unicode White_Space semantics.
- Only family declarations set it. Bitunix declares both fields for both
  slots, so REST and WS login share one trimmed material.

Architecture gate: this extends the shared credential setup choke point.
It is not a per-venue signer or normalization workaround. MEXC adoption still
waits for shared RequestContext actual-release pacing; Phemex separately waits
for raw/protocol expiry projection. Current queue #1734 -> #1736 -> #1735 -> #1728
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

Implementation checkpoint: the shared pipeline is implemented without enabling
MEXC signing. Full focused engine/crypto/Binance/registry/host tests733 pass;
standalone MEXC+dispatch-signing provider groups9 pass; SDK groups23 plus
typecheck/lint and Rust strict Clippy/fmt/bans pass. Disabling only the native
policy override reproduces three behavior RED regressions, restored to GREEN.
Two SDK RED cases reproduce stale primaryComplete for NEL/ASCII whitespace.
Independent correctness/adversarial reviews are clean across all13 source/test
paths. CI/replay proof and actual intervening landing integrations remain open.


## Owner extension: required fields and absent-only fallback

The owner assigned required-passphrase validation to this same declarative seam
before OKX/Bitget adoption. Add a non-secret per-slot passphrase field policy:
optional (default), requiredForPrivate, or requiredWithPrimaryFallback. Declare
OKX's four aliases and Bitget's three aliases once in construction-rules.json:
primary requiredForPrivate, secondary requiredWithPrimaryFallback. This does
not enable their dispatchSigning admission. Required checks apply only to an
existing complete effective key+secret slot; no policy creates a slot.

A missing secondary passphrase may resolve to the primary passphrase reference
inside trusted setup. Explicit empty remains empty and rejects; it never falls
back. Missing/empty fallback also rejects. Raw publicDataOnly bypasses primary
required checks, as before; secondary validation remains active even in public
mode, matching both venues' existing start admission. Partial primary material
keeps its prior public behavior. No trimming of passphrases, secrets or IDs.

Validate raw material synchronously inside the trusted SDK/native/JSON setup
path before opaque installation. Keep required/fallback resolution generic,
including a shared opaque-reference helper for externally provisioned identities:
refs can prove presence, not nonempty provider bytes, so recipe Present insertion
still validates material atomically. Local fallback aliases the installed primary
ref rather than copying passphrase material into a venue. Pass explicit setup
public/private context through the JSON/WASM path; do not infer it from stripped
options. Errors remain sanitized with existing layer-specific contracts (native
construction InvalidInput has no command outcome); caller objects unchanged
and owned buffers are wiped on rejection.

Retain passphrase-only primary input so a complete secondary slot can inherit
its existing material without inventing key/signing refs. Setup mode uses
explicit public intent (including publicMode.requested precedence), never
pre-setup canSign inferred from stripped keys.

RED/GREEN tests cover missing/empty/own/inherited and passphrase-only-primary
passphrases, primary public
bypass versus unconditional secondary enforcement, both trusted entry paths,
opaque fallback, incomplete/absent slots and Unicode/byte preservation. Existing
MEXC trim/default/custody controls remain. Retitle PR #1738 for the shared field
policy and rerun focused/strict checks, independent correctness/adversarial
review and fresh CI. Current source queue #1735 -> #1728 retains priority.

Review corrections: a present publicMode with null/omitted requested ignores the
legacy publicDataOnly flag, matching final construction; only requested:true is
an explicit bypass. Secondary requirements also remain active in the generic
SDK policy. Preserve historical JSON null-as-None fallback semantics (including
secondary passphrase:null), while explicit empty strings never inherit. Non-null
malformed material still rejects. Each boundary has a targeted regression.

Expanded implementation checkpoint (unmerged): 749 focused native tests and
14 standalone MEXC+dispatch-signing provider tests pass; SDK52 plus typecheck/lint,
strict Clippy/fmt/bans pass. Targeted controls reproduced native publicMode
precedence rejection and both SDK parity failures before correction. Independent
full correctness and Rust adversarial reviews are clear after fixes. Source/CI
proof must be refreshed after actual preceding merges. Hosted API compatibility
bridge now precedes Bybit #1728; this setup core remains separate and held.
