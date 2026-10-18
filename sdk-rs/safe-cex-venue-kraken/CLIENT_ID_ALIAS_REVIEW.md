# Spot client-ID callable alias

`binding::rt::SpotIds` is the boxed, mutable, zero-argument module capability
installed by `spot_client_ids`, shared between Spot exchange instances and
consumed by the real `TradingHost::spot_client_id`. It returns a generated
32-character hexadecimal String or the entropy supplier's `JsCoercionError`.
It is not an input decoder: its String is output, and its FnMut environment is
live generator state. The maintained native alias engine independently checks
its compiler-expanded identity, Serde availability and conversion front doors.

The TypeScript source creates `customAlphabet('0123456789abcdef', 32)` once at
module scope in `krakenspot.exchange.ts`, and `placeOrder` calls that generator
before posting `cl_ord_id`. No generator is constructed per placement.

`tests/support/accepted_client_id_alias.rs` installs the production generator
in two actually constructed Spot exchanges sharing one runtime, HTTP host and
Store. It observes the actual boxed callable, entropy refill lengths and state
across both receivers, then checks three signed AddOrder wire bodies and their
returned transaction IDs. The separate error witness checks the native thrown
name/message and that failed generation prevents any order HTTP request.
Only the terminal entropy source and HTTP responses are controlled.

`scripts/client-id-alias-contracts.mjs` uses fresh authenticated compiler,
artifact, macro-origin and source claims to run the maintained native alias
engine. Its negative mode executes the same actual alias without the contract
and requires the named missing-classification failure. No alternate type,
blanket label or generic profile is substituted.

`scripts/client-id-alias-controls.py` corrupts the actual host consumer to
replace the generator output with a constant, then separately swallows entropy
failure. Both controls must compile and fail the exact native witness; each
restores the original source bytes and requires that witness to pass.
