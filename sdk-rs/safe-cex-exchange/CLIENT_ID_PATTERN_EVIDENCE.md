# Client-order-ID pattern compatibility

The generated [inventory](tests/client-id-pattern-inventory.json) contains 20
actual venue call sites and 11 distinct flagless JavaScript regex literals.
`scripts/client-id-patterns.mjs` reads the current TypeScript sources, checks
that inventory, and evaluates all 880 oracle cases in Node's actual RegExp
engine. Regenerate with `node scripts/client-id-patterns.mjs --generate`.

Five sites use dot-length patterns, three use `\S`, and twelve use bounded
ASCII character classes. Orderly's `/^(?!-).{1,36}$/` needs negative lookahead,
which Rust's regex crate does not support. JavaScript's flagless dot patterns
count UTF16 units: 18 emoji fit the 36-unit bound, while 19 do not. JavaScript
`\S` rejects U+FEFF but accepts U+0085. The current patterns reject trailing
line terminators.

`pattern_client_order_id_rule` therefore accepts the original pattern display
and a supplied predicate. It preserves that predicate without silently
translating engines. Venue callers must implement the predicate's JavaScript
semantics; simple character-set and length predicates are preferable where
applicable. The core API does not promise Rust regex compatibility. Its string
surface is Rust UTF8, so tests cover representable Unicode scalar strings.

`core_client_id_patterns` proves every current pattern is expressible by this
API: ASCII patterns use character-set and range predicates; dot patterns explicitly
count UTF16 units and reject JS line terminators; `\S` uses JS's whitespace set.
The ignored live test reads the freshly evaluated Node oracle, not a frozen
expected-results file. CI must execute it with `--include-ignored`.

Both tests pass in the actual crate using `cargo test --locked -p
safe-cex-exchange --all-features --test core_client_id_patterns -- --include-ignored`.
Earlier isolated rustc runs also compiled the actual production client helper
source and runtime contract against Cargo-built dependency artifacts while the
workspace lock update was pending.

Negative controls, followed by restored passes:

- Removed an actual venue inventory row: the source coverage check failed
  with `client-ID source inventory is stale`.
- Changed UTF16-unit counting to Unicode-scalar counting: the live Node oracle
  test failed on the astral-character boundary.
- Removed U+FEFF from the supplied whitespace predicate: the live Node oracle
  test failed on JavaScript whitespace behavior.
