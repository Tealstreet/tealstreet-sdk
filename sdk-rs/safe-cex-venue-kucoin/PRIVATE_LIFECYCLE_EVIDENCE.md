# Private subscription and open algorithms

Moved **4 pending to ported, 0 to divergent**: futures/spot private #subscribe
and onOpen. Actual fresh branch parent:
17b6bcd0db760cfb175d1f5e21e581b5d29bd2a5. Tomllib over all twelve manifests:
67 / 39 / 85 -> 71 / 39 / 81. No unreviewed feature ancestor was adopted.
Implementation gate 5ec7cc1a98b44e7bc79a4ef05ddffd313544e723; refined native
pointer-identity gate/control SHA 0603b88a34140af7f6a7cc454499c5b33a825394.

Scoped all-target KuCoin check exited 0 separately at both gate SHAs. Focused
ws_helpers filtered to ping::private_socket:: exited 0 separately at both SHAs:
three tests passed, nine filtered out. Tests set the isolated TypeScript path,
--jobs 2, one cargo at a time. Extractor at 5ec7cc1a98 exited 0 with inspected
7341 symbols / 512 sources and no drift. Source/manifest inputs to extractor
are unchanged at the pointer-only refinement SHA.

The callback contract preserves Date.now per frame, exact ordered sendWithLog
bytes and synchronous throws; supplied operations own future/background work.
Futures sets subscribed true before the first clock/send and preserves it on
failure; guarded synchronous reentry sends no extra frame. Six independently
pinned ASCII futures topics and two spot topics avoid a dynamic UTF16 surface.
No disposal/connection guard is added to subscribe. Sixteen declared profiles
stop at spot / initially subscribed / fail-at-five, with 29 independently
expected frames. Every decoded native field, complete error and Rc pointer
identity precedes any source bytes.

onOpen has only an entry disposal guard. Futures resets subscribed before
subscribe; both call subscribe then ping, then optionally initiate a snapshot
without awaiting completion. Spot passes batch size 10; futures passes no
argument. Sixty-four declared profiles stop at spot / disposed / initially
subscribed / reconnecting / snapshot throw. Native call order, retained flag,
complete synchronous errors and pointer identities precede live comparisons.
Two native OPEN-core compositions pin six/two frames and private flags.

These rows port the local algorithm contract. Concrete socket-class assembly
remains pending. Existing non-OPEN core reconnect error ownership is unresolved
and is not discharged by injected operations or OPEN composition. Snapshot
future rejection is host-owned; it is not converted into a synchronous return.

Four compiling controls at 0603b88a34 changed each first subscription topic,
futures onOpen reset, or spot snapshot batch size. Named native assertions
failed 101, each source restored byte-for-byte and exact test passed 0; none
survived. Full command/marker table is tracked in
safe-cex-venue-okx/EU_WORKER_POLICY_EVIDENCE.md. Static sub-agent review has no
remaining findings and ran no gates. Not run: full venue/monorepo suites,
workspace compilation, other targets, Rust parity suite, host/wasm Clippy,
wasm build or CI. Final integration merge-down proof follows before push.

Clean merge SHA 728877aada2fd3c7376ef942fc2541dc5d309673, actual integration
second parent f3189bc39dbbb104821ef45cdcc838c99a63c174. Scoped src/tests/
scripts/manifests diff against 0603b88a34 exited 0, byte-unchanged. Shared core
inputs changed; fresh KuCoin all-target check and the three focused private
lifecycle tests both exited 0 at 728877aada. Fresh extractor also exited 0
with inspected zero drift. Controls remain bound to 0603b88a34. Tomllib
against the merged parent still gives 4 pending->ported and 71 / 39 / 81.

Last fetch merged exact integration FETCH_HEAD
3e4e053b47fbfa44475712b68fdf53c23f5071cd at clean merge
b36154c325fef129b4175c7aba6f3580fa44e4c9. Owned src/tests/scripts/manifests
still match control SHA 0603b88a34 byte-for-byte (scoped diff exit 0).
Incoming shared websocket getter caused fresh checks: KuCoin all-target check
exit 0 and three focused private tests exit 0 at b36154c325; extractor exit 0,
inspected zero drift. Tomllib against that fetched integration still gives
four pending->ported, zero divergent, 71 / 39 / 81. Controls remain measured
at 0603b88a34; the next delivery commit changes evidence only.
