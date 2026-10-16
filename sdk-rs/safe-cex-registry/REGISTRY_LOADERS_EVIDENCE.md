# Executable registry loader slice

Gate tree: `493f6409122921db5ee3f4aee5b7644bb247fdbd`. Actual integration parent: `ccc6b553a45033e903488df180dd9ac1523b4edb`.
Fresh branch: `feat/rust-core-registry-94`. The only branch-specific ancestor
before implementation was this lane's replayed WIP `73069665ca`; its loader
bodies were reviewed against the source. No unrelated unreviewed branch was used.

## Row movement (tomllib)

Against the actual parent: **0 pending->ported, 94 pending->divergent**.
Two previously ported descriptor const rows were reclassified to CHOSEN divergent
because their mapping now includes executable registry capabilities.
The fixed eleven assigned manifests contain 127 rows:
parent 94 pending / 22 ported / 10 divergent / 1 not-applicable;
gate tree 0 pending / 20 ported / 106 divergent / 1 not-applicable.
Enumeration stopped at iteration 1, the eleven named manifests in
`parity/scripts/core-p4a-inventory.py`; this is a census, not whole-project proof.
The 94 newly implemented functions are 47 web loaders and 47 native loaders.

## Measured exits at the gate tree

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-registry --all-targets --jobs 2`: **0**.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-registry --test loaders --jobs 2`: **0**, 2 tests passed.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript node sdk-rs/safe-cex-registry/tests/loaders.cjs`: **0**. The actual source initializers execute; only platform import/require is injected. Source assertions precede observation serialization.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **0**; output read: 7341 symbols / 512 files, would update 0 manifests, 0 added, 0 orphaned.
- `python3.11 sdk-rs/safe-cex-registry/scripts/loader-controls.py`: **0** after two controls completed serially. Native wrong export: compiled **101** at `REGISTRY native require and export before future return`, restored **0** (2 tests). Deferred web import: compiled **101** at `REGISTRY web import before poll`, restored **0** (2 tests). Both edits were exact-byte restored; no survivor in these two attempted controls. This enumeration stopped at control 2.
- `git diff --exit-code -- sdk-rs/safe-cex-registry/src sdk-rs/safe-cex-registry/tests`: **0** after controls. `git diff --check`: **0**.
- Independent tomllib transition and provenance/bucket checks: **0**.
- Existing `sdk-rs/target/debug/parity-check` binary: **1**, stopped at integration-owned `safe-cex-webworker/src/twap_manager.rs` (`thread_local!` requires checked-in generated declarations). This is an observed blocker, not a green Rust correspondence/invention claim. No checker weakening or unrelated workaround.

The extractor writer was run before commit `bb02afcd1c`: **0**, updated exactly
2 manifests with 0 added/orphaned. An earlier uncommitted test check exited
**101/E0599** because ExchangeName has no `as_str`; native enum decoding replaced
that test call. The final committed gate tree above compiled and passed.

## Behavioral scope and CHOSEN boundaries

Each production entry is called by the native gate. Independent expectations are
47 entries per platform and 94 compared entries, with exact source-derived module,
export and order. Assertions inspect native constructor capability identity,
missing export None, live namespace lookup, original Rc error identity, and host
call order. Loaders never invoke constructors. A separate callable capability test
pins ExchangeName, key/secret/testnet, Store identity, one invocation and constructor
refusal propagation; it does not prove successful venue construction.

Explicit module hosts own platform loading and concrete constructor factories.
Rust typed Result failures replace JS throw/rejection channels. Caller-polled
futures replace Promise microtasks; dropping an unpolled web continuation can
cancel its export lookup while a discarded JS Promise continues. Typed callable
exports exclude arbitrary JS values and thenables, and Rc<dyn Error> excludes
arbitrary JS thrown values. These are declared CHOSEN differences, not hidden
parity claims. No concrete venue constructor row or facade factory assembly is
credited. Generic facade factory wiring remains separate.

Read-only sub-agent review checked all 47+47 literal mappings and architecture;
its test count finding was fixed in `493f6409122921db5ee3f4aee5b7644bb247fdbd` and confirmed resolved. The reviewer
ran no Cargo or mutation gates; those measured exits above were run by this lane.

No workspace build, full monorepo suite, host/wasm clippy, wasm target, CI rerun,
existing unrelated registry data tests or registry:check gate was run for this
slice. The query transport/exchange source was unchanged from the parent (diff
exit 0); retained-token decode and its no-Value-shim boundary are untouched.

## Final integration merge preservation

Merged integration `57a115c476b3a9ecc391145fec5e2492aeb1f006` down as `e4e2ed1874deae563cf6d22221dbca2aeca79e83`.
`git diff --exit-code 493f640912 HEAD -- sdk-rs/safe-cex-registry/src sdk-rs/safe-cex-registry/tests`: **0**, byte-identical.
The two registry manifests were also byte-identical (diff **0**). Checked Cargo
manifest/lock inputs, dependency crates (exchange, transport, store, types, crypto,
num, candle-cache), source registry modules and extractor inputs had no changed
paths. No affected Cargo/native gate rerun was warranted. Existing measured
exits remain attached to their actual code SHA, not relabeled as new executions.

At merge SHA `e4e2ed1874deae563cf6d22221dbca2aeca79e83`, required extractor check was rerun: **0**,
7341 symbols / 512 files, would update 0 manifests, 0 added/orphaned (output read).
Tomllib census rerun **0**: 127 assigned rows, 0 pending / 20 ported /
106 divergent / 1 not-applicable, iteration 1 fixed eleven-file floor.
