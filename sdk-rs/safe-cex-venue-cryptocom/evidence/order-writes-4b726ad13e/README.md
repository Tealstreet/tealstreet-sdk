# Earlier merged scoped gates

Measured SHA `4b726ad13e0ab6b2ff4ab5cd6dbbd89f2d0d5009`, integration parent faee40c0372da23d6558ad3da511c44a22b05331. Actual fresh branch parent198f871b9b. Own source/tests diff0 from corrected witness cafdd2d601; changed shared WebSocket/Cargo.lock/package/parity inputs prompted all seven sequential scoped gates. Exact commands/env/exits and logs in gates.json.

| Gate | Exit | Raw log |
|---|---:|---|
| venue-tests | 0 | venue-tests.log |
| check | 0 | check.log |
| host-clippy | 101 | host-clippy.log |
| wasm-clippy | 101 | wasm-clippy.log |
| fmt | 0 | fmt.log |
| writer | 0 | writer.log |
| extractor | 0 | extractor.log |

Full scoped venue suite passed eighteen integration targets plus library unit target, enumeration stops tests/ws_policy.rs. A draft evidence helper expected19 integration files and failed its inventory assertion before writing a README; that was an evidence-script error, not a gate failure. The measured full Cargo command exited0. Raw output preserved.

Host and wasm clippy each101, blocked by inherited safe-cex-transport/src/tealstreet.rs:157/177 Arc-with-non-Send/Sync diagnostics. Neither is a lint pass; later venue diagnostics may be preempted. Scoped cargo check completed0.

Writer0 with Updated0 manifests/0added/0orphaned; extractor0 with Would update0 manifests/0added/0orphaned. Output text read. One Cargo at a time, jobs2, locked, TypeScript path set.

Latest integration subsequently introduced socket modules and changed the full crate bytes; these gates are historical and not falsely attributed to that later merge. Affected final gates are attached separately. No workspace build/suite, live venue API, wasm runtime or CI run.
