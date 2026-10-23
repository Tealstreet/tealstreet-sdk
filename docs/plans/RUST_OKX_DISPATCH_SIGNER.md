# OKX dispatch-time signer adoption

Plan, 2026-10-22. Fresh base: actual landing
`7c2d573ca91eddf92096d7be084f0955898fd232`, including setup-field policy
#1738, provider seam #1731, hosted bridge #1745 and Bybit #1728. Half-2 #1757
is still unmerged and is not an ancestor. Native recipe implementation starts
on landing; delegated hosted admission integrates the actual shared bridge
after its owner-ordered window. Keep #1757 CI watches active.

1. Replace retained raw primary/secondary signing credentials with opaque
   identities. Reuse the shared provider construction seam for all four aliases
   (global/EEA, live/demo). Preserve mandatory primary passphrase and the
   secondary rule: absent inherits primary, explicitly empty rejects. Do not
   enable dispatchSigning until HTTP and both static WS login paths migrate.
2. Stage the four existing HTTP builders (list, account get, ordered writes,
   primary client-ID lookup) through one selection helper. Preserve exact
   method/path/query/body, streaming ownership, request route and unsigned
   public reads. HTTP primitive is HMAC-SHA256 over ISO UTC milliseconds +
   uppercase method + exact path/query + exact body, output Base64(raw MAC).
   Use DispatchContext.stamp_ms(); obtain key/passphrase through shared Present
   operations, never retain material in venue state.
3. Migrate private and business WS login to send_signed, preserving primary
   identity, login-before-ping FIFO, JSON escaping and string seconds timestamp.
   Sign seconds + GET/users/self/verify with the same primitive; receipt records
   the actual emitted seconds in milliseconds. No session token or signed
   upgrade is required. Keep auth supervision and all timers in raw Cx.now.
4. Route typed provider failures to the actual owned continuation. An entirely
   unsent write stays NotSent; earlier accepted steps retain financial effects
   and existing aggregate evidence. No fabricated native auth rejection,
   mutation retry, empty financial snapshot or definitive lookup absence.
   Preserve shared history-owner partial-result semantics and generation fences.
   Any missing shared outcome/timing seam is reported before a local workaround.
5. Owner decision: peel declared output encoding and atomic passphrase
   presentation into a small shared PR after #1757, with OKX and Bitget tests.
   This family PR consumes that actual extension before hosted admission.
   Current HTTP formatting assumes hex/key-only, and the #1757 delegated capsule
   cannot present passphrases. Trusted provider setup owns material/ref fallback;
   no SDK receiver/options storage or external wire change. Extract the exact OKX request interceptor to a standalone signing
   module (no venue class construction) and use the owned provider for HTTP
   and WS under all four aliases. Preserve TS header precedence and WS
   extraData.timestamp from prepared work. Test hosted browser/keyless HTTP
   and private/business WS per alias; missing/explicit-empty passphrases and
   required-secondary RemoteSigner work refuse before I/O/callback. Shared
   extension/admission waits actual #1757; do not copy its unlanded source.
6. Add independent native byte oracles and regressions for both credential
   slots/passphrase boundaries, all aliases, primary lookup egress, delayed
   release/calibration, provider refusal, ordered partial writes, stale account
   cleanup, history ownership and both socket channels. Test standalone
   okx+dispatch-signing without Binance and actual rebuilt WASM SDK setup.
7. Add the caller contract to RUST_CUTOVER_CALLERS.md in this same PR.
   Include setup, secondary/passphrase fallback, hosted and worker provisioning,
   direct/EEA/demo routing, and unresolved caller gates. Shrink the signing
   legacy ratchet to the exact scan; classify pure work descriptors separately.
8. Run focused OKX+Binance/engine/registry/host tests and strict checks, independent
   correctness/adversarial review, full CI and authenticated replay comparison
   across the eight OKX cases. Preserve complete IDs/signatures/financial fields;
   never widen declarations. Integrate actual shared advances normally and use
   an exact own-only separate guard before merge under the owner's queue.

Architecture gate: adoption of the existing shared engine signer/provider
choke point, not a venue-local clock, credential custody, scheduler, outcome
ledger or retry implementation. No Phemex-style raw/protocol release-expiry
projection or MEXC-style physical-send pacing dependency was found in OKX.
Public/private readiness and caller transport routing remain explicit. No live
capture is claimed; tape/native/actual-WASM evidence will be reported separately.


## Native recipe checkpoint

HTTP and WS SignStep implementations are verified under `cfg(test)` before
switching the production builders. They stage opaque references, exact
canonical bytes and atomic key/MAC/passphrase presentation; no local crypto or
credential state is added. Nine library tests pass, including the fixed WS
vector, dispatch-time fractional/ISO stamps, complete legacy header order,
and native path/query mismatch rejection before either provider or I/O.
Independent review caught and cleared the path-binding and header-order gaps.
Production credential replacement, owned failure wiring, registry activation,
caller section, ratchet shrink and hosted alias proofs remain to implement.
This checkpoint does not claim that OKX dispatch signing is enabled.

## Native builder checkpoint (not ready to merge)

The four real HTTP builders and private/business login now use the existing
shared dispatch provider. Production options retain opaque identities only;
trusted setup owns raw material and passphrase fallback. Independent production
review is clear, including strict lookup/history refusal and preservation of
accepted bracket effects after a later signer refusal. Existing native fixtures
enter trusted provider setup, preserving their financial and lifecycle assertions.

Standalone OKX registry/WASM feature declarations select dispatch signing without
requiring Binance. The compiled bridge flag is staged for native construction;
this branch is deliberately not ready to merge: hosted HTTP/WS admission and
all-alias actual-WASM positives depend on the separate shared presentation PR.
No keyless/hosted support claim is made by this checkpoint.

The legacy scan includes OKX signing modules and decreases its exact allowance
from 517 to 516. The twelve newly scanned matches are six independent test-oracle
sites, two primitive work-builder descriptors and four test setup/oracle sites;
none grants production credential custody or local signing. Source-independent
CI replay equality, remaining delayed/socket/account/history regressions, hosted
proof and actual shared-input integration remain readiness gates.

Checkpoint verification: 274 OKX/registry native tests, 266 Binance tests,
strict all-target OKX/registry Clippy, SDK typecheck/lint and 59 focused setup
checks pass. Disabling typed owned signing-error handling reproduces three
runtime RED assertions (first write, later accepted bracket effect, strict
lookup); restored source passes all four actual-builder tests including delayed
reprepare. Independent adversarial review recomputed the full ratchet: exact
516 matches/516 allowances, all non-OKX entries unchanged, no stale allowances.
Actual-WASM hosted and replay evidence remains pending, not inferred from native
fixtures or the SDK constructor-only setup tests.

## Hosted completion on landed presentation

Actual #1763 `d05c5ede4b86409e1a8d9798e64b79b1c1c888f4` is integrated at
`442622ac4f4`. The construction conflict preserves every incoming declaration
and adds only the four OKX dispatch flags, under the owner's explicit ruling.
Cargo's existing registry build regenerates its embedded construction table;
the construction-codegen and OKX signer targets pass all 12 controls.

1. Extract the existing OKX HTTP preimage/body/header construction into a small
   standalone codec shared by the TS interceptor and Rust hosted capture. Keep
   decoded qs queries, exact serialized bodies, ISO timestamps, public bypass,
   testnet headers and native header precedence. The codec never owns secrets.
2. Admit the four aliases to registry hosted captures and declared keyless
   RemoteSigner policy. Extend the existing prepared WS metadata codec with
   OKX's `{timestamp}` envelope, preserving each alias label. Provider-owned
   passphrase Present operations remain separate from HMAC bytes. Primary-only
   remote signing and required-secondary refusal remain shared policy.
3. Keep private PayloadSigner unsupported: the existing OKX TS interceptor
   requires a local secret and does not implement that combination. Do not
   invent a REST-only account lifecycle in this signing port. Browser signing
   and keyless RemoteSigner HTTP/private+business WS remain supported.
4. Add per-alias actual-WASM/SDK HTTP and WS positives and deterministic codec,
   setup, error and exact legacy-wire controls. Check passphrase absence/empty,
   opaque refs, disposal and no-network capture boundaries. Update the caller
   section and family guidance in this PR; retain all other policy entries and
   exact ratchet counts.
5. Run native OKX/registry/Binance tests, focused SDK tests, typecheck/lint and
   independent correctness/adversarial review. Actual-WASM execution, replay
   equality and performance remain CI gates, with no local release build.

Architecture gate: this adopts the landed provider, capture, setup and codec
policy seams. It introduces no credential custody, retry ledger, clock,
socket policy or request admission mechanism. The socket core is a separate
draft and is not an input to this branch. Landing is held for #1769.

## Hosted implementation checkpoint (CI pending)

The standalone codec, four-alias registry admission and exact legacy WS metadata
are implemented. Eight original HTTP assertion failures now pass. Correctness
and adversarial reviews found and cleared a generic UTF-8 part-boundary
regression and a missing actual-native-WASM login oracle. The latter now starts
the SDK/engine and drives both private/business login and real signed HTTP; it
is authored for CI, not locally executed against an older binary.

Local verification: 485 full OKX/Binance native tests, 40 focused recipe/registry
controls, one independent fixed-vector hosted-capture test spanning all four
aliases, 23 full-registry construction controls and 146 focused SDK tests pass;
SDK typecheck and lint pass. The exact legacy ratchet is 516 actual / 516 allowed
(517 before the native port), with every non-OKX declaration retained. Strict all-target registry/OKX/Binance Clippy passes. Actual landing
integration, actual-WASM CI and replay proof remain readiness gates. The socket draft and allocator are separate workstreams.

## Lane takeover status (claude-4yjlw0, 2026-10-23)

Checkpoint `d8bbc31da9a` confirmed as the newest work: 10 commits ahead and 345
behind landing `73d04380dd9`. PR #1773 head `1dc55f06624` is historical. Native
and hosted adoption are implemented; CI, actual-WASM and replay evidence are
stale. The branch edits `registry/src/hosted_signing.rs` and the registry
`hosted_signing`/`dispatch_provider` tests, which overlap #1839.

Next:
1. Hold until #1839 is actual (agreed order). Then porcelain-merge one pinned
   landing, preserve incoming entries, and default every Identity literal
   this branch adds (`extra: Default::default()`).
2. Reverify the #1814 error rows against current source (50001/50013/50026
   Unknown with accepted IDs; missing 50100/50101/50103-50105). Close them
   through shared Context and code tables only.
3. Focused OKX/registry/Binance tests, Clippy/fmt and SDK setup checks. Push to
   #1773 for per-alias actual-WASM positives and CI. Refresh the CALLERS text.
4. The WS-seconds suffix-codec generalisation comes with the Bitget PR (second
   consumer). Do not refreeze OKX for it.
