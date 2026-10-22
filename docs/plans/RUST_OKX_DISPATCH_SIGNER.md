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
