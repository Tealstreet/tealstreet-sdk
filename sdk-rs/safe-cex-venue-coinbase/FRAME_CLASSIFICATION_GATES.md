# Coinbase public frame classification

Moved **4 pending -> ported, 0 -> divergent**: the classification union, notification classifier, test-response classifier and raw frame classifier. Python 3.11 tomllib enumerated through all 16 Coinbase manifests and 426 rows at authoring parent `95e6665431234b770e820f5f06dab0e3ad386d76`: **329/17/80 -> 333/17/76** (ported/divergent/pending). Each moved row has a real Rust target; scoped mapping checked them. This fresh branch uses reviewed integration directly, without a foreign unreviewed ancestor. Census is separate from behavior proof.

The native carrier preserves JSON number semantics (including overflow, underflow and negative zero), UTF-16 strings/keys including lone surrogates, JavaScript own-key ordering, and shared object/array payload identity. Closed native enums preserve classification, reasons, channels, suffixes and outcomes. Context validation precedes raw-input validation; own `method` routing precedes own `id`. Source helper mappings retain existing implementations and add the genuine native overlays.

Boundary: parsed JSON values and plain typed JSON context records. Arbitrary JavaScript getters, prototypes and symbol properties are outside this typed API. Non-string raw inputs share the source unsupported-type result and are represented by `None`.

## Gates at the authoring SHA

Exact commands, measured exits and log hashes are in `FRAME_CLASSIFICATION_GATE_RESULTS.json`, bound to **`089ea68e49c0908197e5ce7c0af530731d7e7de3`**.

| Gate | Exit | Measured result |
| --- | ---: | --- |
| Scoped Coinbase Cargo check, all targets, jobs 2 | 0 | Zero errors; finished dev profile |
| Affected native/oracle tests, jobs 2 | 0 | 18 passed, 0 failed/ignored; 7 completed test binaries |
| Extractor check, explicit Node/Yarn | 0 | 7341 symbols / 512 files; zero updates/additions/orphans; output inspected |
| Scoped native mapping | 0 | 426 rows / 540 public Rust items; real targets validated |
| Sequential mutation driver | 0 | Four compiling mutants caught; four restored runs passed |

Affected test selection: library, frame_classification, json_key_scanner, decoder_boundaries, public_frames, public_test_error and validate_public_context. Oracle environment: `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`. Cargo operations were sequential. The extractor writer ran after the manifest edit and updated one file; native file inventory was serialized from actual target paths.

## Native assertions and discrimination

Native enum fields, UTF-16 units, numeric bits, own-key order and nested native data are asserted directly before any wire assertion. Flat source projection tokens avoid harness recursion limits; native values are never re-encoded or roundtripped. Explicit Rc identity and mutation assertions cover payload object/array aliases, primitive copies and error-data aliases.

Independent fixture plan stops at **31 notifications, 64 responses, 69 raw frames (164 classifier cases)**, plus **4 alias/copy probes**. Fixture content SHA-256: `62b02b0753be9f326ec91450cff51b756f4e320c6a94c1f8f3440493166a93d3`. These are coverage floors, not exhaustive claims. A precommit harness projection failure was corrected by flattening tokens, without changing production decoder limits; it is not a passing gate at the authoring SHA.

| Compiling mutation | Named assertion | Mutated / restored exit |
| --- | --- | --- |
| Skip desired-channel membership | `decoded native classification kind` | 101 / 0 |
| Reverse timing-difference comparison | `decoded native classification kind` | 101 / 0 |
| Change duplicate-key reason to invalid JSON | `decoded native unknown reason` | 101 / 0 |
| Detach payload object identity | `native payload object alias` | 101 / 0 |

Control enumeration stopped after these four; no observed survivor. Each failure occurred in the named test assertion, not compilation. Source, tests and scripts were independently byte-restored to the gate SHA (`git diff --exit-code`, 0). Read-only subagent review found no blocker within the stated boundary and confirmed four targets; the reviewer ran no Cargo, native tests or controls.

Not run: full Coinbase all-target test suite, workspace builds/tests, monorepo suites, host clippy, wasm clippy/builds, CI, or live Coinbase network operations.

## Integration merge-down

Merged integration `9d6f40b96faafcfa73e19ffcfb2825e4d3e78444` into code SHA `c4b78ac61c20d0381424b9d14577df8a99b7792c`. Kept both Coinbase and Bitget exemption groups; TOML parsing and unique-target validation passed. Coinbase source/tests, scripts, Cargo configuration, scoped dependency crate paths and Coinbase source oracle were byte-unchanged from `089ea68e49c0908197e5ce7c0af530731d7e7de3` (`git diff --exit-code`, both commands 0). The only lockfile edit adds an existing base64 dependency to unrelated ApexOmni. The original scoped check, 18 affected tests and four controls carry; Cargo was not rerun after this merge.

At the merge SHA, extractor check exited **0**, with zero-update output inspected, and scoped mapping exited **0** (426 rows / 540 public Rust items). Exact measurements are tracked in `FRAME_CLASSIFICATION_MERGE_GATE_RESULTS.json`. Final integration-relative tomllib census stopped at 16 files and 426 rows: **329/17/80 -> 333/17/76**, a true gain of **4 pending -> ported, 0 -> divergent**.

Final fetch merged integration `c9f9322af452ee70b2d311e479b28aafbf9a82ee` into code SHA `f36bb3cc3219595d7678671117a09fdd49c172aa`. Whole Coinbase source/test comparison to authoring gate SHA exited **0**; scoped test/oracle/dependency/configuration/manifest inputs also remained unchanged from the first merge. Measured gates carry across this second merge, and final extractor independently exited **0** with zero-update text inspected. `FRAME_CLASSIFICATION_FINAL_GATE_RESULTS.json` binds those measurements. The final tomllib comparison against this integration head remains **333/17/76**, with **4 pending -> ported, 0 -> divergent**.

PR 7537 repair: merged requested integration `4fa402f6fb4890a9ebd0d2ccf585f720c6f3a10a` into `9c7e1e856c43285958a00ce279505bf3dec4655f`; retained both shared exemption groups. Source/test comparison to authoring gate SHA exited **0**; scoped Coinbase dependency/oracle/configuration/manifest inputs unchanged from `f36bb3cc32`, so check/tests/controls carry. Extractor independently exited **0** with zero-update output inspected. No Cargo/test/control rerun. Exact record: `FRAME_CLASSIFICATION_PR7537_MERGE.json`. True gain remains **4 pending -> ported, 0 -> divergent**.
