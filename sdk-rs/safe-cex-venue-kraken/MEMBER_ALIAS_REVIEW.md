# Kraken class-member continuation aliases

Three finite profiles fix `classes::{BodyFuture, NativeMember, BoundMember}` at
the actual `binding::rt::Rt` host. BodyFuture owns a pinned native future yielding
`Result<Rc<Js>, Thrown>`. NativeMember takes a native receiver plus argument cells;
BoundMember takes argument cells with its receiver already captured. Both return
`Result<classes::Return<Rt>, Thrown>`, whose alternatives are a native value or a
body future. They are dispatch/continuation capabilities, not wire decoders.
Expected Rust types are independently authored; no other host is classified.

The source Futures class declares `fetchOHLCV` as an async arrow, with lexical
`this` and the original options. Rust production `classes::initialize` captures
the receiver into BoundMember, `ClassHost::arrow` adapts its return into native
value/Promise, and `ClassHost::member` dispatches NativeMember over the explicitly
supplied receiver. The real charts implementation returns BodyFuture through
`binding/members.rs::future`. No production body change persists in this cohort.

`tests/support/accepted_member_aliases.rs` uses actual constructed exchanges and
real chart clients. Its three named witnesses check held HTTP completion and all
six independent candle fields, dispatch to another constructed testnet receiver
with exact symbol/interval/limit query parameters and no request on the original
client, and detaching the actual initialized arrow then calling it with a foreign
constructed `this` while retaining the original client/arguments/Promise result.
Terminal HTTP/runtime are fixtures; member lookup, arrow construction, dispatcher,
client, options conversion and response conversion remain production code.
This is not a complete method-matrix or socket-prototype claim.

`scripts/member-alias-contracts.mjs` obtains fresh maintained
`acceptedAliasContext`: compiler JSON/check artifacts, authenticated external
roots, macro origins and public source claims. Stock native alias invariant
identity, Serde and conversion probes remain unchanged. Its negative mode
requires all three actual missing-contract failures; positive classification
is counted separately from production decoder execution.

`scripts/member-alias-controls.py` corrupts the actual BodyFuture completion,
NativeMember receiver forwarding and BoundMember argument forwarding one at a
time. Each must compile, fail its own exact named runtime assertion, restore
original bytes in `finally`, and pass. Compiler failures do not count as negative
evidence. No blanket non-decoder rule or new exemption is introduced.

Commands from `sdk-rs`, one Cargo build at a time:

```sh
RUSTC_BOOTSTRAP=1 node safe-cex-venue-kraken/scripts/member-alias-contracts.mjs --expect-unclassified
cargo test --locked -p safe-cex-venue-kraken --jobs 1 --lib accepted_member_aliases
python3 safe-cex-venue-kraken/scripts/member-alias-controls.py
RUSTC_BOOTSTRAP=1 node safe-cex-venue-kraken/scripts/member-alias-contracts.mjs
RUSTC_BOOTSTRAP=1 CARGO_TARGET_DIR="$PWD/target/accepted-inventory" node safe-cex-parity/scripts/accepted-set.mjs --package=safe-cex-venue-kraken --inventory --jobs=1
```

Discovery stays in the separate accepted-inventory target. Full parity index,
extractor check, all-features cancellation/override, and both touched crates'
fmt --check plus all-target Clippy are separate readiness gates. Other Kraken
aliases and owners remain independent obligations.

The selected canonical replay reports three explicit classifications, zero failed
owners and zero decoder runtime results. All three real constructed-Scope tests
pass; all three semantic controls compile, fail their named runtime witnesses,
and pass after exact source restoration. The full inventory clears BodyFuture
and its two member profiles, then reaches the pre-existing
`binding/members.rs::UNSUPPORTED_MACRO_IMPORT_SCOPE` from dispatch's wildcard
import. Ten original aliases remain independently unclassified. Neither clean
full discovery nor whole-Kraken accepted behavior is claimed by this cohort.
