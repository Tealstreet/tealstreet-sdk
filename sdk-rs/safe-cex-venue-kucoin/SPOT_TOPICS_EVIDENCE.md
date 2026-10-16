# Spot topic helpers

Moved **0 pending to ported, 2 pending to divergent** for
KucoinSpotPublicWebsocket#subscribeTopic and #unsubscribeTopic. Both are FORCED
by the required UTF8 topic surface: live D800 is transmitted and observed after
JSON.parse; native String cannot retain it and refuses the whole topic.
Literal ASCII backslash-u and valid Unicode are preserved. Algorithms preserve
connected/disposed guard before Date.now, five ordered payload fields and
complete fallible core send errors.

Actual fresh-branch parent: f436fc816390a8b4556ab7a2fe0ae85072337036.
Measured gate SHA: 3e315f89dd4216ff959fb958bb8aad02f4b8b66d.
Tomllib over all twelve manifests: 67 / 37 / 87 -> 67 / 39 / 85.
Scoped all-target check exited 0. The ws_helpers target filtered to ping::
exited 0, two focused tests passed and seven were filtered out. Extractor check
exited 0, output inspected: 7341 symbols / 512 sources, zero drift.

The forty-eight-case floor stops at unsubscribe / Unicode topic / connected /
disposed / send failure; twelve decoded frames are independently expected.
Native fields, effect order, full errors and surrogate refusal precede wire
comparisons. Source observes materialized length-one D800 after transmission
in both methods, not merely literal escaped ASCII.

The shared fixture adopts landed ExchangeRuntime::monotonic_now_ms; native
close/terminate now update readiness. Static sub-agent review has no remaining
findings and ran no gates. The complete shared sheet, per-row controls and
merge proof are tracked at sdk-rs/safe-cex-venue-okx/PENDING_HELPERS_EVIDENCE.md.
Other KuCoin test targets were compile-checked, not executed. No workspace,
full suite, host/wasm Clippy, wasm build, Rust parity suite or CI was run.

Final integration merge-down was clean at
50366986fcd870f12d0116cc71b7834559432010, second parent
55d8addaf74e5ff2c2da80ab19a03109b6fb9476. Scoped diff against gate 3e315f89dd
for src/tests/scripts/manifests exited 0, byte-unchanged. Fresh KuCoin all-target
check and global extractor check both exited 0 at 50366986fc. Focused tests
and the two compiling opposite-operation controls remain measured at
3e315f89dd: each native operation assertion failed 101, then byte-restored
and the exact test passed 0; none survived. Tomllib against the merged head
still gives two pending->divergent and 67 / 39 / 85. Shared core source inputs
are unchanged; Cargo.lock additions concern Coinbase and Weex only.
