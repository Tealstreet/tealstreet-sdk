# Blofin mode-utility slice

Moved **3 pending rows to divergent; 0 to ported**. tomllib census at code SHA
**31e261356315f83a8ad63fb35555788602cf757d**: **14 ported /31 divergent /80 pending**,
125 rows across7 manifests, stopping at manifest7. Independent starting expectation
83 pending at parent **89ad07ac80f6efc169430a2f94bd59898660e9f9**; these3 source
methods were pending there. The prior STORE/TYPES/UTILS19-row branch is not an
unreviewed ancestor of this fresh branch. Initial merge-down parent:
**7fd5102999754d0a1d95910af1fb540c3bffb94e**.

The slice implements getPositionSide, applyConfirmedPositionMode and
afterMethodCalledInWorker using genuine shared Store/typed cache context, retained
worker inputs, and an asynchronous mode-reader callback. Specific CHOSEN notes
cover explicit host injection, owned snapshots versus JS aliases, excluded
prototype/getter fields, and refusal of truthy unsupported closed-enum mode values.
The full exchange class, REST mode readers and websocket clients remain pending.

Measured gates at **31e261356315f83a8ad63fb35555788602cf757d**:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-blofin
  --all-targets --jobs 2`: **0**.
- `cargo clippy`, same manifest/package/all-targets/jobs, `-- -D warnings`:
  **0**, **host** only.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript
  cargo test`, same manifest/package, `--lib --test modes --jobs 2 -- --nocapture`:
  **0**, **4 native tests** (1 prepared-update unit test,3 live-source tests).
  Actual TS method AST initializers, genuine imported enums and genuine TS
  DefaultStore run in one corpus. Independent floors stop at **48 side cases,
  6 cache-update cases,12 worker cases**. Native fields, IDs, counts, callback
  ordering and error origin are asserted before any wire comparison; no same-codec
  decode/reencode equality is used. Native preparation checks isolate the actual
  StoreUpdate before Store normalization. Ready-only fetch fixtures do not prove
  delayed/pending scheduling.
- `rustfmt --edition 2024 --check` on changed modes source/test files: **0**.
  Not a workspace fmt claim.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`:
  **0**, output7341 symbols/512 files, **would update0** manifests. Writer ran after
  edits and canonicalized1 manifest, then reported0 drift on its repeat.
- Preexisting local `sdk-rs/target/debug/parity-check`, workspace index scoped to
  Blofin only /all7 Blofin manifests /empty exemption file: **1**. Exact findings:
  unchanged `src/position_adapter.rs::{BlofinPositionAdapterDeps,
  map_blofin_position_history,num}` have no manifest/exemption. Scoped
  `git diff --exit-code 89ad07ac80 HEAD --` that file: **0**. No exemptions/targets
  were invented to conceal inherited noise. No current checker rebuild or global
  Rust-parity pass is claimed.

Canonical TS declaration coverage ran at earlier **b55eabb1d577fa4792cfd70943a791daebda5c0c**,
exit **0**, independent source-file floors20 types /3 income /3 history /66 exchange
(4 files,92 declarations). Only a Rust unit-fixture trait-selection correction
followed; its TS sources/manifests/oracle/extractor inputs are byte-unchanged.
This source census and the runtime behavioral corpus are different claims.

Discrimination at **31e261356315f83a8ad63fb35555788602cf757d**:
3 serial production mutations, one per newly mapped row, each named native
assertion failed at runtime **101** and passed restored **0**, stopping at3/3.
Source bytes were restored. Per-control names, call sites, exits and raw logs are
tracked in `modes-evidence/final-controls/`.

**Survivor retained:** at **92b169628c92e54be7d39749a2b3f973eaa95b3e**, removing
`modes.rs` helper's `ids.insert(position.id.clone())` filter survived (**0**), and
restored test also returned0. `DefaultStore::update` performs a second dedupe,
masking the missing helper operation. That original Store-result test remains
useful for final state but is not helper-dedupe proof. The later private preparation
split and native assertion before Store normalization catch the identical mutation.
Original logs/results remain in `modes-evidence/initial-survivor/`; it is not counted
as caught. The first preparation fixture selected a shadowing inherent method;
its broken/restored **101/101 compiler errors** atb55 are retained under
`harness-failure/`, never counted as runtime discrimination. The explicit shared
Store trait call fixes that fixture; production behavior is unchanged.

Sub-agent ticker_review static review bound to exact
**31e261356315f83a8ad63fb35555788602cf757d**: no remaining findings; earlier inherent
method shadowing was missed then corrected. Reviewer ran no Cargo/oracle/control
gates. Full venue suites, full workspace compilation/tests, wasm check/clippy,
network REST calls, CI reruns and global inherited Rust-parity enumeration were
**not run**. Final delivery merge preservation is recorded separately.
