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
