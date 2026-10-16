Isolated proof-admission implementation for parent integration; tracked repository files were not edited.

Base reviewed source: 78e9ea486ec776f6849394fb7fda8cd3eb76dcad.

Proposed files:
- bridge-manifest.mjs: main-only writer, validates raw proof logs before manifest writes; preserves complete qualified target paths; owns Prepared/Awaited and eight public inherent BoundHost methods alongside existing roots.
- bridge-proof.mjs: pure independent 23-control/46-receipt admission contract, exact phase/SHA/command/exit/outcome/marker checks and named raw runtime failure/restoration checks; rejects unsafe/reused log names. CLI rejects symlinks/nonregular files.
- bridge-targets.mjs: bounded alias/struct/function and qualified owner precheck. This is not the canonical Rust indexer.
- bridge-proof.test.mjs: 16 focused tests over independent explicitly synthetic unit inputs, never real evidence directories.

Actual measured Node unit proof, recorded in unit-proof.json with input SHA256 and portable argv:
1. Old count-only/every-row admission logic adapted to the new23/46 floor and substituted in an isolated copy: exact duplicate-receipts rejection test failed exit1 with Missing expected exception (1 test, 0 pass, 1 fail).
2. Restored isolated copy: focused suite exit0, 16 tests/pass, 0 fail.
3. Original proposed files: focused suite exit0, 16 tests/pass, 0 fail.

The initial proof wrapper expected filtered TAP test number2 instead of1 and itself failed after correctly observing Node exit1. The corrected wrapper reran the whole sequence; no failure was dropped. Raw logs and the weak-guard patch are retained. The independent inventory now includes proof-admission-guard before source-noop and stops index22 source-noop. Its Rust wrapper/control was not compiled or run by this child; the23rd receipt pair appears only in explicitly synthetic unit inputs here.

Additional actual Node-only textual check at base source: exit0, 15 independently listed real targets accepted (12 native_bindings roots/inherent items and 3 ReadHost declarations); receipt committed-target-precheck.json. This does not prove canonical-indexer admission.

Not run: Cargo, cargo metadata, Rust tests, writer CLI, extractor, canonical parity indexer, CI. No class-status transition or real Rust control execution is certified by this isolated unit work. Raw receipt checks cannot authenticate intentionally fabricated execution histories; they reject missing, duplicated, mismatched and compiler-blocked receipts under the recorded contract.
