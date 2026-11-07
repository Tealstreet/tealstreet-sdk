# OKX-only socket-core variant (option 1 preparation)

Sam selected option 1 for #1771: OKX uses the shared core and Bitunix remains legacy.
The original #1771 history is retained; its source is replaced by this selected scope.
A fresh integration and CI qualify the updated PR; landing still needs an explicit engine window.
Historical preparation below preserved original #1771 remote head
`dbde7c702e03bddec255b97449b5927bd0812f09` and local source head
`ab6e90d2f83db9a290cfcdbb4b1f58db6c902b3d` unchanged. This new branch starts at
actual #1820 `2b1e35e50b2095c6651344fc5ffe0553ad456f35`.

## Implementation

1. Import the reviewed shared socket core and OKX adoption from local #1771,
   using the conflict-free three-way source integration with actual #1820.
   Preserve every incoming HTTP/Context/command-entry/header/ID/signing rail.
   No source conflict is authorized: stop for the owner if one appears.
2. Leave the complete Bitunix crate exactly equal to actual #1820, including
   its dependency features, legacy callbacks, timers, tests and native wires.
   Only OKX declares the temporary engine managed-sockets feature and registers
   managed feeds. Combined builds retain the runtime legacy path for Bitunix.
3. Bootstrap the new socket legacy inventory from the actual venue source with
   only OKX removed. Bitunix remains explicitly listed. Do not inherit the
   unmerged #1771 inventory that already removed Bitunix; no ratchet exemption
   or growth against a landed manifest is needed. Existing request inventories
   remain incoming-exact. Add the variant plan to the index and describe the
   resulting OKX-only behavior in CALLERS. Preserve incoming docs verbatim.
4. Keep budgets, WASM profiles, benchmark commands and gate thresholds exact
   actual #1820. This variant tests the migration boundary, not a CPU optimization
   or the option-2 redesign. No old memory/CPU PASS is inherited.

## Verification and review

- Check exact Bitunix tree equality, all unowned source blobs, every incoming
  HTTP/header/Context rail and the original remote/local #1771 refs.
- Exercise engine/OKX/Binance and legacy Bitunix native controls, strict feature-on
  and feature-off checks, formatting and request/socket provenance ratchets.
  Keep existing typed-auth, stale epoch, retirement, replay, signed-order, fatal
  callback and cleared HTTP-delivery regressions. The existing legacy Bitunix
  tests are the controls for the deferred family.
- Review final source and feature graph, including combined-build feature
  unification; no Bitunix managed registration may remain.
- Push a separate frozen branch and dispatch the existing unmodified full Rust
  SDK workflow. Inspect all OKX public/private/EEA/testnet/inverse replay memory,
  CPU, size and correctness results and report the exact run/head/job evidence.
  No local WASM performance run, extra diagnostic loop, threshold change or
  standalone artifact-provenance audit is required. Retain all real failures.
- No landing guard, merge or change to original #1771 until the owner chooses
  this option and grants a window. Report CI results even if the option fails.

## Actual command-entry integration correction

The first bounded ordering review found that old managed `Cx::close` returned
before actual #1830's `opaque_entry_activity` mark. Newly introduced no-ID socket
registration, one-shot staging, interest proposals, retirement and stop likewise
could change existing state without spending a fresh command-entry loan. Before
CI freeze, add seven genuine Engine command-callback regression cases, prove
assertion RED against the imported source, then use the existing shared mark at
these managed side-effect entry points (and before the managed close branch).
No new entry state, ID allocator, HTTP root, retry permission or per-venue fix.
These new APIs conservatively spend fresh entry before attempted side effects;
existing suppressed legacy behavior and all incoming HTTP guards stay intact.
Native baseline checks before this correction are preservation only, not final
CI qualification. Integrate actual #1837 before the new frozen CI run.

## Selected scope and landing refresh

Sam selected this option on 2026-10-23. Integrate current landing, keep every incoming
HTTP/Context/entry/host/signing rail, and preserve the incoming Bitunix crate. The
legacy Bitunix socket boundary is a **remove-before-master obligation**, unless Sam
later approves a specific Bitunix endgame. Its presence is tracked in the shrinking
socket family/site inventories and the [master blocker ledger](RUST_MASTER_MERGE_BLOCKERS.md).

GitHub keeps the original #1771 head branch. A history-preserving merge records the
old two-family branch without importing its superseded tree: the selected OKX-only
tree remains exact. The decision/ledger commit follows that merge before push. No
force push, rewritten commit, new ratchet exception, threshold or build-profile change.

Verification: engine/OKX/Binance plus unchanged Bitunix all-target/all-feature tests,
strict Clippy, feature-off Clippy, formatting and all bans/request/socket ratchets;
review the incoming event/HTTP/fatal ordering; then fresh normal PR CI and replay
gates. Request an engine window only when these checks and reviews are clear.

### Existing WEEX bootstrap entry after landed signing

Overseer authorized the existing WEEX private socket reference refresh after
#1855 landed at `6741c1c6609437e32090c76745a56ca6c2262445`. Its native
`cx.connect` became `cx.connect_signed` under the same socket owner, while
remaining on the legacy socket lifecycle. In `venues/weex/src/account.rs`,
replace only the single-count fingerprint
`3aee64a1b26b8bb71902dbf435c1e8d963ba8378d797896a1348ae6f6de2cad5` with
`f9aa006cf88eec01bc93cf70d941c3b65c5fb1ed55dedd3cc56e871f8c516fde`.
All other manifest maps and the family inventory remain exact; no checker,
venue source, site count, budget or build profile changes. This is the
current landed reference in the initial socket bootstrap, not a new caller.
