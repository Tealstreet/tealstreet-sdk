# Reviewed validator contracts

The focused runner retains 13 reviewed functions: eleven public V3 validators,
`required_stats_string` and the legacy `deserialize_present` helper. All 768
cases remain unchanged. The ten private-read and seven regular-write validators
are retired with their unused caller closure; see ACCOUNT_WRITE_REVIEW.md.

The live-source script executes maintained TypeScript bodies and imports. Its raw
corpus covers missing/null, booleans, whitespace, radix/exponent/decimal strings,
signed zero, integer limits, overflow, underflow, arrays and records. Public Number
coercion, parameterized minimums and borrowed identity remain observed. Concrete
String/Value presence observations make no universal generic-deserializer claim.

Native adapters retain numeric bits, string/boolean values, optional presence,
record fields, array slots, borrowed identity and actual errors. They never use
DTO serialization to supply native observations.

The runner preserves complete compiler discovery, actual Cargo artifacts,
authenticated macro origins and selected-source checks. Its compiled control
short-circuits each copied function after contract validation and requires a named
native mismatch. Original production bodies remain byte-identical.

Run the source check from the root and native replay from `sdk-rs`:

```sh
node sdk-rs/safe-cex-venue-woo/scripts/accepted-validator-oracle.cjs --check
cd sdk-rs
CARGO_INCREMENTAL=0 RUSTC_BOOTSTRAP=1 CARGO_TARGET_DIR=target/accepted-inventory node safe-cex-venue-woo/scripts/accepted-validator-contracts.mjs
CARGO_INCREMENTAL=0 RUSTC_BOOTSTRAP=1 CARGO_TARGET_DIR=target/accepted-inventory node safe-cex-venue-woo/scripts/accepted-validator-contracts.mjs --control
```
