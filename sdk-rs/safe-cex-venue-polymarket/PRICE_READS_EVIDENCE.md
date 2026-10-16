# Polymarket numeric-price and REST read bodies

**8 pending → ported; 0 pending → divergent.** Polymarket triple: **50 / 18 / 73** (ported / divergent / pending), total141. [Census](evidence/price-reads/census.json) checks an independently specified eight-symbol set across all seven venue manifests against requested integration parent **`ac9c9470293c896c3aadbc039a5b0ab70679d890`**. Census is distinct from runtime proof.

Fresh branch `feat/polymarket-rest-price-reads-ac9c94` starts directly at that parent, with no unreviewed private lane work as an ancestor. Before gates, integration `93ddd6b77b9ce509b8fa8ee77247a9fbf4dfc472` was merged down.

## Callable method bodies

Eight individually mapped functions in src/price_reads.rs: getMidpoint, getMidpoints, getPrice, getLastTradePrice, getPriceHistory, getOrder, getBalanceAllowance, deriveApiKey. Notes name each exact free-function rename and every anonymous request/response value target. The official scoped checker verifies all off-pending targets without venue exemptions.

Requests start synchronously before the future is polled. The caller retains the underlying operation; the returned future borrows its completion, so abandoning observation does not cancel the host operation. Original native errors propagate unchanged. Scalar prices preserve parseFloat prefix behavior, including trailing text, incomplete exponents, infinities, NaN, signed underflow/zero and lone-surrogate termination. Midpoint POST rows are newly mapped objects detached from the input array. Midpoint results preserve the unique own-entry order/keys of Object.entries/Object.fromEntries.

Price history keeps all five parameter keys even when optional values are undefined. Falsy history (undefined/null/false/±0/NaN/empty string) returns a fresh empty array; truthy empty arrays, objects, strings and numbers retain native identity/value. Order URLs preserve unescaped UTF-16 concatenation. Balance signature type remains a Number, including signed zero and nonfinite values. Derivation headers preserve exact spelling/order and Number.toString results.

Explicit boundaries: hosts supply ordinary typed string price fields, ordinary unique own-entry snapshots, ordinary history records and non-thenable response data. The midpoint Vec is an own-entry projection, not an arbitrary JS object/prototype implementation. Getters/proxies, thenable assimilation, the API class/factory, authentication/interceptors/retries, real network and Axios wire serialization remain outside these body ports. Remaining pending rows stay pending. UTF-16 replacement is used only inside parseFloat: a lone surrogate and U+FFFD both terminate its allowed prefix, while request/key/output units remain intact.

## Gates attached to SHA

All final exits below were measured at **`c2761975a3534059edab975248bc626af400f0c2`**. [gates.json](evidence/price-reads/gates.json) records exact commands and oracle environment, with tracked raw logs. Cargo commands ran sequentially.

| Gate | Measured result |
| --- | --- |
| Scoped cargo check, Polymarket, all targets, locked, jobs2 | 0 |
| Scoped Polymarket tests, locked, jobs2 | 0; 51 passed, zero failed; doc tests ran zero cases |
| Scoped cargo fmt check | 0 |
| Official venue-only invention/status checker | 0; 141 rows, 102 public Rust items; zero exemptions |
| Actual Yarn parity:extract:check | 0; inspected output reports zero manifest changes, added or orphaned symbols |
| Eight isolated production controls | 0; shared ten-test baseline0, each mutation101 and exact restored-test0 |

The earlier committed attempt at `a983398b4009f2b72ee6f09b4467cbe5bc9d40c4` measured check0/test0/fmt0, then scoped-parity1 because notes did not name the exact primary Rust rename. Extractor and controls did not run in that attempt. [Earlier gates](evidence/price-reads/initial-gates/gates.json) and raw logs retain those exits. The notes were corrected and a static review's independent source row-cardinality assertion added before the final gates above. Development oracle initially failed1 on a cross-realm prototype assertion, corrected to compare the VM's own Object.prototype. A dirty native draft failed compilation on closure ownership and was corrected to borrow host operations; its wrapper did not retain the Cargo exit, so no numerical draft compile-exit claim is made.

The first control attempt at `c8becc40235a9327bb4cfd526c1c8a766279b1d6` exited1 at control4/getLastTradePrice: replacing the result with zero left response unused, so -D warnings rejected compilation101 at src/price_reads.rs:168, and restoration passed0. This was a non-discriminating control finding, not a named runtime catch or survivor. First three controls had runtime0/101/0; last four had not run. [Failed attempt](evidence/price-reads/failed-control-attempt/gates.json) and its summary/raw logs retain caught=false. The raw summary floor string was the intended eight-control loop bound, not the reached bound; [actual-floor receipt](evidence/price-reads/failed-control-attempt/actual-floor.json) records the actual fourth-control stop explicitly. Revised last-price, order and derive mutants retain variable usage so runtime assertions can execute; the latter two warning risks were corrected before their first mutation runs. Final controls below all ran at the new SHA.

## Native assertions and controls

The oracle extracts and runs original TypeScript bodies and the original endpoint table, with no network or credentials. It captures request values, own key order, UTF-16 units and native Number bits before JSON output. Native assertions directly inspect request fields, result values/bits, error/array/object identities and two-way mutations. Expected counts, endpoint/field keys, native inputs and numeric results are independently specified. Scalar response fields are changed while awaiting completion, and assertions require the later values. Each source mapped token row independently contains exactly one token_id field. NaN is checked by native is_nan and a source NaN tag; no NaN payload-bit parity is claimed. No native re-encoding, roundtrip or request-wire assertion is used in this new suite.

| Isolated production fault | Named native assertion | Baseline / mutation / restored |
| --- | --- | --- |
| getMidpoint: read price instead of mid | native price result:getMidpoint:native Number bits | 0 / 101 / 0 |
| getMidpoints: rename token_id to token_ids | native mapped row key:getMidpoints | 0 / 101 / 0 |
| getPrice: rename side parameter | native parameter key:getPrice | 0 / 101 / 0 |
| getLastTradePrice: return zero | native price result:getLastTradePrice:native Number bits | 0 / 101 / 0 |
| getPriceHistory: invert history truthiness | native original array identity:getPriceHistory | 0 / 101 / 0 |
| getOrder: omit order ID suffix | native request URL:getOrder | 0 / 101 / 0 |
| getBalanceAllowance: increment signature type | native numeric parameter:getBalanceAllowance:native Number bits | 0 / 101 / 0 |
| deriveApiKey: substitute signature for address | native header units:deriveApiKey | 0 / 101 / 0 |

[Control summary](evidence/price-reads/controls/summary.json) and 17 raw logs retain shared baseline and each broken/restored run. Each isolated fault compiled and reached its named runtime assertion; finally restoration returned original source bytes. No survivor was observed in this eight-control floor.

Coverage floors: **8 methods × 16 inputs × 2 outcomes = 256** source/native method calls. One abandoned-observer call and two fresh-fallback calls bring each passing corpus to **259** calls, captured in one Node invocation per process. Per-method floor stops at input15/rejection, iteration31; full corpus fixture255 is deriveApiKey/input15/rejection. Midpoint key enumeration stops at sixth key; header enumeration at fourth key, POLY_NONCE; mutation enumeration at control8, deriveApiKey; census at seventh manifest, polymarket.ws-public.ts.toml. These are finite floors, not complete venue proof.

Static sub-agent review verified production, native tests, oracle, mappings and all eight control anchors. It requested an explicit non-thenable domain, repeated fresh fallback proof and independent source mapped-row cardinality; all were addressed. Final review found no remaining actionable issue and stopped all readers before controls. Reviewer ran no gates.

No workspace compilation/tests, host clippy, wasm clippy, CI, MEXC gates, real network or full client-pipeline/wire gates were run. No such results are claimed.

## Final merge preservation

Last pre-push merge includes integration `c390d6df02c16810ac5b813f11306c858dd2704e` at comparison head `fc17ba56a417ab5f81aad2b9709a17e4e6b8b31b`. [Receipt](evidence/price-reads/merge-preservation.json) records scoped gate-input diff exit0: venue src/tests/scripts/manifests/Cargo.toml, TS venue/log severity inputs, seven dependency sources/Cargo.toml, Cargo.lock, workspace settings, checker/extractor and yarn.lock are unchanged. The aggregate comparison including package.json exited1: its sole change appends --test public_lifecycle to scripts.venue-oracle:kucoin. Independent JSON comparison proves all other package fields, dependencies and our extractor script unchanged; raw diff is retained. Polymarket exemptions remain independently expected0, and the scoped checker supplies its own empty exemption file. Post-merge tomllib census remains50/18/73. Prior scoped check/test/format/parity/control exits remain attached to `c2761975a3534059edab975248bc626af400f0c2`; no fresh scoped run at this merge head is claimed. Fresh global extractor at `fc17ba56a417ab5f81aad2b9709a17e4e6b8b31b` exited0 with zero manifest changes. The following evidence-only commit changes no gate inputs.
