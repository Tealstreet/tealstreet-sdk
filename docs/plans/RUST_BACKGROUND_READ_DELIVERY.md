# Exact background read delivery

Plan first, 2026-10-23. Base actual #1830
`0001c78eeb2d37d65c1f494ee53f303d66cd7c96`. This is the small shared
prerequisite for one-leg background instruments/ticker reads identified by the
Kraken Futures lane. It is separate from whole-plan admission and joined work.

## Contract

Add `Cx::validate_background_read_http_delivery(&Context) -> Result<(), Error>`.
It first delegates to `validate_http_context`, preserving the existing issuer,
registered-handle and immutable deadline checks. It then requires exactly
`Owner::Background` and `Operation::Read`, and compares the supplied Context with
the active `io.http_delivery.context`. Wrong owner/operation returns Invalid;
absent or mismatched delivery returns Revoked. The function is read-only and
allocates no IDs, contexts, timers, actions, command owners or retry authority.

The existing command root/leg validator and completion rules remain unchanged.
Session is a distinct owner and must not pass this Background-only check. The
session owner cleared this additive method/test-module region; session issuance,
generation validation and retry policy remain in its own shared core. Matching
Background explicitly also rejects future other owner variants by construction.

This proves current delivery ownership, not native response validity, snapshot
freshness, symbol selection, startup generation or successful HTTP status. A
failure callback has a delivery identity but no response data. An adapter must
validate its native response and publication freshness separately, and publish
synchronously after the gate. No command terminal event is produced.

## Missing-proof regression and verification

The baseline probe uses the suggested existing `validate_http_context` call in
real engine-managed Background callbacks. Assertions require a wrong live
Background peer and an unrelated socket callback to produce zero native/Store
publication. They must fail before the new gate exists. This demonstrates the
missing shared contract, not a regression in an existing venue adoption.

Commit the baseline probe before production changes. After implementation, only
its authority call changes to the new public method; assertions remain exact.
Use managed prepare/bind/claim and HTTP response/failure delivery, with a positive
matching-leg control. Additional controls cover exact expiry, foreign issuer,
retired handle, Command Read/Mutation rejection, duplicate/late response and
unchanged resources/actions. Keep current command read-delivery tests intact.

Engine fixtures represent single-request background reads; they are not claims
of native Kraken or other venue adoption. Native consumers own their later
serializer/cache/caller proofs. Run the focused controls, complete engine suite,
strict all-target/all-feature Clippy, feature-off controls, fmt and bans/ratchet;
normal CI (including its replay gates) and a separate guard follow publication.
A separate full artifact-provenance collection is not a merge requirement under
the owner's current speed rule. Actual #1820 must be
integrated before the final shared-engine window if it lands first.

## Scope and gates

Only one additive Cx method and a cfg(test) module hook in
`sdk-rs/engine/src/request_context_registry.rs`, a new focused test module, this
plan/index and a CALLERS section. No lifecycle dispatch, queue, signing, Session,
history, venue runtime, workflow, harness, package or inventory change.

Before commit/publication, independently review source and all incoming
preservation. No per-venue policy copy or generic logical-validation fallback.
The helper is an opt-in authority check; it changes no native wire/result shape
and requires no new SDK/NX order-error handling.

## Baseline checkpoint

Plan `8696e74ad1a` precedes test checkpoint `0764dd3b5d7`. Against actual #1830
with the probe delegating only to `validate_http_context`, the real managed
callbacks reproduce three assertion failures and four compatibility passes.
The failures are wrong-live-peer native publication, socket borrowing and the
absent-delivery authority matrix. No compile/fixture failure is counted as RED.
Raw local log: `/tmp/codex-background-delivery-original-red-final.log`.

## Historical implementation verification

The following receipt describes the original implementation checkpoint before
the eighth Execution compatibility control and the current landing integration.

The new read-only gate changes only the baseline authority call and comments;
all regression assertions remain byte-for-byte unchanged. All seven controls
pass, including the original three architectural assertion failures. Full
engine/Binance/Kraken native verification passes 1,193 tests across 53 binary
and doc-test targets, zero failures or ignored tests; the engine unit target
contains 601 tests. Feature-off controls pass 7/7. Strict all-target/all-feature
Clippy and formatting pass. Bans and 13 legacy-ratchet controls pass; inventory
and checker files are unchanged. Root typecheck/lint/test tasks pass 44/28/27
from unchanged cached inputs. The first bans invocation had an incorrect local
script path; the corrected `sdk-rs/scripts/check-bans.sh` exits zero. That
tooling correction is not a product failure or an assertion RED.

Independent plan and source/test reviews are CLEAN. Removing the additive
method and test hook reproduces the actual #1830 registry verbatim. Session
coordination confirms the narrow region is clear and Session must be rejected.
Architecture: no per-venue workaround. Native venue adoption and whole-plan/
observer authority are not implemented by these engine fixtures.

## Ownership transfer and current landing integration

On 2026-10-24 codex-ih35js took ownership of #1856. Integration commit
`ef257d1cdf5bfec4a9ce82074c9c42ec809f323b` genuinely merges landing
`c473342d48363a057c98c59292abb957462f3590`. The engine source merges without
conflict. The shared CALLERS appendix keeps all incoming text first and the
existing Background section once. The original method, test hook and eight
current test bodies remain unchanged; no venue, inventory or dispatch changes
are introduced. The scope remains the original additive shared authority seam.

At that integration, the eight focused Background controls pass with default
features. Strict all-target/all-feature engine Clippy, bans, the request-context
checker and its 15 controls pass. The earlier seven-control and native-suite
counts above remain historical evidence. Fresh exact-head CI and the shared API
owner review are pending; this receipt does not claim merge readiness or native
MEXC adoption. The intended MEXC consumer must separately fence streamed native
side effects, response validity, freshness and startup continuation.
