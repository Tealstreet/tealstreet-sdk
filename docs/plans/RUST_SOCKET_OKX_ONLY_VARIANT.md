# OKX-only socket-core variant (option 1 preparation)

User-authorized preparation only; no merge. Preserve original #1771 remote head
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
