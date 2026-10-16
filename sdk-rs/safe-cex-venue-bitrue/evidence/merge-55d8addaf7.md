# PR 7380 urgent merge-down

Measured clean code SHA: `402ce968a8790321796101f0f4fe1fc46a4679b4`.
Integration `55d8addaf74e5ff2c2da80ab19a03109b6fb9476` was merged down cleanly, with first parent `28e0c6039e58418e8f953e7aff4cc2c579cdaa49`. The branch retains unmerged PR 7380 work as ancestry. The earlier merge from f436 had one Store CLAUDE conflict; both sides were kept without rereading its guidance. No conflict occurred in the55 merge.

Bitrue and shared Store/Exchange/Types/Num/Transport/Crypto source, plus Bitrue tests, are byte-unchanged from28e0 (comparison exit0 in the attached JSON). The borrow-guard repair survives unchanged. The earlier regression fixture adaptation adds only timestamp:None, preserving all native assertions.

The latest control temporarily substitutes pre-fix account_reads.rs from1226, with its exact patch retained. The exact patch is stored losslessly as a JSON string to preserve diff context whitespace. The dirty control is explicitly labeled with its baseSHA and variant, not presented as a clean-tree run. It exited101 with the named runtime RefCell panic during the unrelated ticker write while old fallback results remain alive. Production bytes are restored in finally before the clean regression (exit0) and Bitrue all-target check (exit0). The native regression compares independent balance/ticker values and pointer identities, and reaches replacement/reset after the fix; there is no roundtrip or wire comparison. The control's floor stops at the ticker write, and the restored test stops after reset; no exhaustive Store-write coverage is claimed.

Rows moved: **0**. Independent tomllib census compared Bitrue manifests to previous pushed3ea80 and found no status differences. Bitrue remains15ported/25divergent/67pending. Enumeration stopped after6manifests/107rows at BitruePublicWebsocket.listenOrderBook. The existing PR's two pending->divergent rows remain inherited.

Earlier measurements at28e0: four-package all-target check0, eight targeted Hyperliquid state tests0, retention regression0, Bitrue suite0, Store suite0, strict host clippy0, strict wasm clippy0. Its broader fmt check exited1 on inherited module ordering in Store and Hyperliquid lib.rs; no formatter failure is hidden. At969, the broader check initially exited101 on integration's missing monotonic_now_ms test-clock method, then28e0 added the fail-closed mock method. Partial logs and exact-SHA JSONs are retained. These are earlier runs, not latest402 measurements.

Not rerun at402: full Bitrue/Store suites, host/wasm clippy, formatting, canonical index, full Hyperliquid or Exchange suites, workspace compilation/suites, wasm runtime, old control collections, exchange network calls, or CI. The user prioritized the latest merge and control over broad repetitions. Latest scoped check and extractor runs are listed below from measured output. All Cargo ran sequentially with compiling jobs2. Extractor output is read, not inferred solely from shim status.

## Completed latest-SHA runs

| Gate | Exit | Variant |
| --- | ---: | --- |
| old-guard-control | 101 | ONLY account_reads.rs replaced with pre-fix1226b95769 source; exact mutation patch retained |
| retention-restored | 0 | clean tracked tree |
| bitrue-check | 0 | clean tracked tree |
| extractor-writer | 0 | clean tracked tree |
| extractor-check | 0 | clean tracked tree |

Writer output: Updated0 manifests, 0added, 0orphaned. Check output: Would update0 manifests, 0added, 0orphaned. Exact commands and logs are in gates-402ce968a8.json. Native sub-agent static review of402 confirmed both merge parents and byte-unchanged repair/dependencies, with no blockers; reviewer ran no Cargo. Delivery adds evidence only; no latest gate exits are attributed to earlier runs or to a modified production tree.
