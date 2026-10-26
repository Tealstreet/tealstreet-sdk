# Final timestamp and boxed-ticker evidence

Measured implementation/test SHA: `e571841d02a35e095366d947f2d6367fdce4d7f4`.
Boxing origin SHA: `738110c7040b9b9acd7081f26b8211620ee9c8fe`.
Actual inherited parent: `74d0b79e1dddb094fd147ddf161cd4ed6b189a40`;
its first parent is `9ccdd3788ce530265a98c73f1f9428a3d51a36d7` and second
parent `e09e62df48ed594e7e91403674c839a9f870e545`. The inherited timestamp
work was unreviewed by this agent when work began. The attached static review
now covers the timestamp/boxing slice, not the entire inherited branch.

`results.json` records exact commands, exits and measured SHA. All nine outer
commands exited 0: scoped all-target check, three one-test targets, two control
runners, host clippy, wasm clippy and extractor. Cargo commands were sequential.
The timestamp control runner internally uses jobs=1; other scoped cargo gates
use jobs=2. SAFE_CEX_TYPESCRIPT_PATH is recorded with the results.
The extractor output says 7341 symbols, 0 updates, 0 added, 0 orphaned.

Five deliberately changed production variants compiled and failed at named
assertions with exit 101: timestamp/quoteVolume field swap, signed-zero erasure,
timestamp patch removal, timestamp alias copy removal, and tagged gateway enum.
Each runner restored its sources and passed its focused targets (exit 0, 1/1).
Individual mutation and restored logs are included. No survivor occurred in
these five iterations; enumeration stops at control five.

The first supplemental fixture at `c244a26e4ee42201becb3dda54e044b531376ab2`
failed after native checks at the wire assertion: serde_json distinguishes
integer and f64 number storage. `failed_fixture_results.json` and its log retain
that measured failure (101) and its preceding scoped check (0). This accidental
fixture failure is not counted as a deliberate negative control.

The nine-file tomllib census at inherited HEAD and at integration
`2bc83d78d4f821c4b116d082d02c7b5a2595ffc5` is (ported, divergent, pending) =
(8, 0, 87), 95 total rows. Eight instrument mappings already existed; the assigned
87 rows were pending. Census expectations come from the assignment, not output
length. Enumeration stops at file nine; this is no whole-workspace census.
This ticker/test/evidence slice moves 0 pending rows: it finishes the explicitly
assigned boxed-ticker lint and review. The next slice must port pending rows.

No workspace build, full crate suite, wasm runtime test, host/wasm full workspace
lint, binding-generation freshness check, or networking/clock-host test was run.
This document-only evidence commit does not change the measured Rust/test SHA;
no exit measured at e571841d02 is claimed as measured at its later document SHA.
