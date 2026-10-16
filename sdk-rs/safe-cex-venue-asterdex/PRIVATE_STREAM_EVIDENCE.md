# AsterDEX private stream — eight remaining rows

Moved **1 pending→ported, 7 pending→divergent**. ASTERDEX is 126 ported /87 divergent /18 pending over231 rows. Independent tomllib census iteration1 stopped after eight files and eight named changes; seven distinct bucketed notes and real targets.

Actual start parent: `d8bc6fc533787b072aa5aaad1ee3b0ee09f8681e` (fresh branch; no stacked unreviewed lane ancestor). Code/gate input: `3d0472ed17f7e1199b8eb7b24214966feff31ebe`. Branch: `feat/rust-asterdex-private-stream-8`.

| Gate | Input SHA | Exit |
|---|---|---|
| AsterDEX scoped check, all targets/jobs2 |3d0472ed17|0|
| Exchange scoped check, all targets/jobs2 |3d0472ed17|0|
| AsterDEX all-target tests, include ignored/live source oracles, two test threads |99bf4c4116|0|
| Extractor check, output inspected: would update0/add0/orphan0 |3d0472ed17|0|
| Independent eight-row tomllib census |3d0472ed17|0|
| Six-family compiled mutation runner |3d0472ed17|0|

The successful full suite is measured at99bf4c4116. PRIVATE_STREAM_TEST_PRESERVATION.json proves all native/test/fixture/oracle/TS/Cargo/lock inputs unchanged to3d0472ed17; the only changed script is the separately executed Python control runner. A redundant full-suite rerun at3d0472ed17 was canceled143 and is not claimed to have passed.

Six compiled controls (close wrapper, connection URL, receipt tracking, order reduce-only, balance PnL, account event time) each failed a named assertion with **101**, restored exactly, then passed with **0**. Iteration2 stopped after these six families; zero uncaught mutations. Iteration1 stopped after two controls: the URL mutation was caught by an earlier native log assertion, but the runner expected a later URL assertion and exited1. Its original finding is retained in PRIVATE_STREAM_CONTROL_PIN_FINDING.json; the assertion pin was corrected without dropping the mutation. Exact commands, outputs, SHA and byte hashes are in PRIVATE_STREAM_EVIDENCE.json and PRIVATE_STREAM_CONTROLS.json. Compile failures and canceled earlier gates are explicitly excluded.

Source floor:25 distinct cases, six families,42 operations/67 native state snapshots; additional tests pin disclosed prototype/string/type boundaries, a Store-mutating listener and Array.forEach's captured length. Native models and events are checked before wire/request/URL assertions for every family; comparisons never re-encode models. The oracle executes the unedited private class, actual order-type mapping and real DefaultStore, with Base I/O/time/RPC sinks injected. Native sub-agent review fixed callback borrow hazards, reentrant removal timing, formatting and oracle fidelity gaps; the reviewer ran no gates.

Divergence is per row: injected native host assembly, owned event snapshots, closed model values, currency prototype assignment, lone UTF-16 surrogates and owned cancellation/fallible dial. Constructor ordering is ported. Exchange startup and API construction remain separate pending rows. Retained-token query decoding was not changed.

Not run: workspace or monorepo suites/builds; host or wasm clippy; wasm checks/tests; full shared Exchange runtime tests; live HTTP/WebSocket connections; managed-relay end-to-end connections; browser/worker event-identity scenarios; CI reruns. Direct and managed relay call the concrete shared on_close wrapper, established by source inspection; this is not a relay I/O execution claim.

Merge-down preservation will be recorded separately in PRIVATE_STREAM_MERGE_EVIDENCE.json. Evidence-only descendants do not change code/test/script inputs; cargo exits above belong to the explicit code SHA.

Post-merge gates at `80552af6a481f58b9cf81b858377d44150ef25dd`: AsterDEX check0, Exchange check0, full AsterDEX suite0, extractor0 and census0. Integration parent `f364cee096ff92584c157cad8b96f12b92cf3a09` contains221commits since starting head. AsterDEX source/tests/scripts/manifest are byte-unchanged; shared Exchange/transport/Cargo inputs changed, so these gates were rerun. The shared conflict retained both the close hook and all incoming reconnect methods. Original six mutation catches remain measured at3d0472ed17, not rerun at merge. Full outputs and preservation exits are in PRIVATE_STREAM_MERGE_EVIDENCE.json.

Final merge `be4925e039d81b3e3acd91cc9bc241552ddefb59`, integration parent `00cafd8f46790c3e283decca55b6618bf8ea4d97`: source/test and extended input diffs0; reachable lock graph unchanged. Post-merge cargo gates remain measured at80552af6a4 and were not repeated on unchanged inputs. Final extractor/census measured atbe4925e039 both0. PRIVATE_STREAM_PUSH_PRESERVATION.json records commands and exits. The following evidence-only commit does not change these inputs.
