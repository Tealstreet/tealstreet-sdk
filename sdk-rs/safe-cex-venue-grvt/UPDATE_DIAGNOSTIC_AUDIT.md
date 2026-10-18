# GRVT update diagnostic divergence audit

Assigned row: `GrvtExchange.updateOrder` in `grvt.exchange.ts.toml`.
Verdict: **FIX**. The original constructed member projected its input to
`UpdateOrderOpts` before counting keys. `{amount:2, extra:7}` therefore logged
`fieldCount=1` instead of the maintained source's `Object.keys(update).length=2`.

`binding/methods.rs` now reads the raw enumerable own-key count before typed
projection. `order_controls::update_order_with_field_count` receives that scalar
and owns the unchanged validation/cancel/replace algorithm. The existing public
pure typed helper retains its API and derives its count from represented fields.
No second raw update bag or relaxed shared model is needed.

The named `constructed_update_counts_all_own_raw_keys_before_validation` probe
uses the actual GRVT constructor and installed `updateOrder` member, real shared
logging/error reporting and the existing primitive HTTP/runtime harness. It
covers one, two and three own keys, including an extra null field. Each input
logs before the unchanged missing-time-in-force preflight failure; no cancellation
or placement HTTP is sent. This test compiled and failed against the original
member, then passed after the production repair.

`tests/update-diagnostic-control.py` independently breaks only the production raw
key capture. It requires the named runtime assertion to fail after compilation,
restores the exact source bytes and requires GREEN. Existing order-control source
oracles and typed-helper tests remain intact; their historical pure typed-input
boundary does not authorize loss in the real constructed member.

Validation on this change: named constructed probe and independent control/exact
restore pass; `exchange_binding` and `order_controls` pass; strict all-target
Clippy, pinned formatting and documentation tests pass. The all-features shared
cancellation inventory passes 6 tests (one existing discovery-export ignore),
and its Node source tests pass.

The complete crate oracle was attempted with the required absolute TypeScript
and Ethers 6.17 installations. It stops at the existing
`divergence_records::selected_record_boundaries_are_observable` whole-record
surrogate-refusal assertion, which is stale after shared decoder PR #1041. The
shared decoder owner is migrating that expectation separately, preserving the
source corpus and its 22/7/1,782 pins. This PR changes no record decoder or that
expectation. Full crate GREEN is not claimed.
