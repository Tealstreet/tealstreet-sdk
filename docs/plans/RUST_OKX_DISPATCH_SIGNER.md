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
