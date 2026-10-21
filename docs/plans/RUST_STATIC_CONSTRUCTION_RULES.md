# Compile shared construction rules

Generate the Rust construction-rule table from the existing shared JSON during
Cargo builds. Preserve the TypeScript resolver, its JSON source and the existing
parity test. Runtime evaluation must retain missing/null fallback, empty condition
semantics, every venue name, Aster version behavior and explicit-private errors.
Remove runtime JSON decoding, rule allocations and the rule HashMap/OnceLock.

Validate the generated schema and condition rendering with focused Rust tests;
run registry construction checks, Bybit/Binance tests, fmt, strict Clippy, bans
and the existing JS construction parity test. CI must measure optimized WASM and
run the unchanged replay/size gates. Do not change budgets or build WASM locally.

The build script validates the shared schema and emits immutable condition slices
and an exact venue match into OUT_DIR. serde_json is now a host build/dev
registry dependency. Missing/null fields retain default fallback; explicit empty
conditions remain true and an empty inner group remains false. All venue rules
remain available even in single-family builds.

The CI WASM fingerprint now includes the shared JSON, which is a real Rust build
input. The added dirty/committed-input regression failed against the old input
list. Independent review found no behavior changes. Optimized size savings and
the existing compiled TS/Rust parity matrix are gated by fresh CI artifacts.

CI run39588823047 measured the Bybit family at1,277,533 bytes, SHA256
`e824d59f61c1f66620591607c632102a5f00d93626059807d539a2faf6d37df4`.
That is31,464 bytes smaller than landing389's1,308,997 bytes, with65,955
bytes remaining under the unchanged1,343,488-byte budget. Bybit replay passed.
The local compiled parity matrix requires a generated combined WASM package,
which is unavailable locally; fresh combined-engine CI runs it unchanged.
