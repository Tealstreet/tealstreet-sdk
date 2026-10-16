# Bitrue integration validation

The workspace gate revision is `7b4b2980f98c0540b88f009ea6ff9c926c6bba0a`, a clean merge of delivery `02a85d6e62f6562c28b69f73de01e66feaa36a7a` and advertised integration `49a942e76143d6d1d44567edff59c12669d92627`. Each command below was run sequentially from `sdk-rs`, with `RUSTFLAGS=-Dwarnings`, two build jobs unless specified, and TypeScript supplied externally. Gate executions belong to this revision; historical scoped gates and nine controls belong to `9f1be06793a067d64a079a9dbfebae84774cd01a`.

Local tools were rustc 1.99.0 (`b940084d7`), host `aarch64-apple-darwin`, and Node 26.7.0 (version commands exited 0). The first queue used Node 26.7.0; the six Node-sensitive commands below were then repeated with CI-pinned Node 24.20.0. CI runs on Linux; local Darwin executions do not claim Linux execution.

Integration subsequently advertised `dcb88b8e64efc8da8ed831388c9adaec2609b77f`; PR7967 and PR7972 both merged at 14:08:45 UTC. That fetched revision includes Rust source, Cargo.lock and workflow changes and is not part of this measured 7b unit. These exits do not cover it, and 5,323 is not quoted as its diagnostic count. Merge-down validation is a separate step.

## Measured gate exits

All 13 commands in the first queue exited 0. Each Node 24 follow-up below also exited 0. Bitrue tests passed 60 / failed 0 / ignored 0 in each runtime profile. The policy audit passed 14 tests. Hashes identify sanitized local logs, not uploaded artifacts. The command working directory is `sdk-rs`.

| Initial queue command | Exit | Seconds | Log SHA256 |
| --- | ---: | ---: | --- |
| `cargo fmt --all --check` |0|18.44|`e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`|
| `cargo check --workspace --all-targets --locked --jobs 2 --keep-going` |0|2125.54|`56ea768d390f7643e2dec273e316a0274984ecada183ebc66563b647f4890659`|
| `cargo clippy --workspace --all-targets --locked --jobs 2 --keep-going` |0|2071.57|`f379837c5165c884ba62231325c4062ca35f029bb3a6639e547d47c16e2d008a`|
| `cargo build --workspace --locked --jobs 2` |0|318.56|`e7338a960538b2d52d47b0eca672e30e87011aa13ebc7f0beafe49e18cb22710`|
| `node --test ../.github/scripts/rust-wasm.test.mjs` |0|27.67|`5f24812bdd7ec91a2d49ea1c68d2800fa60bc7f729be88b25c8a9cbb2a27a6c6`|
| `node ../.github/scripts/rust-wasm.mjs --build` |0|381.14|`77c92f8bce5ffeac886288833adcb9719f53d6a50addf858ab861307f87961f9`|
| `node ../.github/scripts/rust-wasm.mjs --clippy` |0|5069.31|`55d06296d23af6db9ee5dd4309ce2ff0e6b0d5fd65cf3115e2950a313defc3cb`|
| `cargo build --locked -p safe-cex-exchange --target wasm32-unknown-unknown --jobs 2` |0|61.08|`55531b992cf618628dbd69bff1b4a0f96c7f860ffc3151e9742c3139446936e8`|
| `cargo build --locked -p safe-cex-exchange --all-features --target wasm32-unknown-unknown --jobs 2` |0|14.11|`7ac210e7b33197e496dfea07a50e825afc305cc865d46da17e4022d1f22f721a`|
| `node safe-cex-ffi/scripts/setup.mjs` |0|0.09|`f9eedb9803429c8cbc38911d530cc5b7d2a083661f0ce5f007797f0e6a1173be`|
| `node safe-cex-transport/scripts/warning-wasm.mjs` |0|49.11|`bc677b2675230a828246dad5575b055a4a32c7f68d5b28fbe31d2fc88588e630`|
| `cargo build --locked --target wasm32-unknown-unknown -p safe-cex-types -p safe-cex-num -p safe-cex-store -p safe-cex-crypto --jobs 2` |0|23.05|`d123813f21c3f682edd818dc5dd3a69b06572ae43a3de0e3f6cefded1d2db515`|
| `cargo test --locked -p safe-cex-venue-bitrue --all-targets --jobs 1 -- --include-ignored --test-threads=1` |0|758.36|`f9ac6b203ade86c6e638a538ae223aa0f8d5a6b3900d74f33656bc5657727a3f`|

| Node 24.20.0 follow-up command | Exit | Seconds | Log SHA256 |
| --- | ---: | ---: | --- |
| `node --test ../.github/scripts/rust-wasm.test.mjs` |0|1.96|`8d104108992dbb317f50a7dff9078fa201711c1b18bf77497ece47f643ca3446`|
| `node ../.github/scripts/rust-wasm.mjs --build` |0|1.41|`225d27f910a35b8676d2bb74b7205f6c61e4684314783da17405029de147e896`|
| `node ../.github/scripts/rust-wasm.mjs --clippy` |0|2.83|`63f5bee7c18a1b5b8cb6e7067cc75737a0ecc93e590f427cde6d0441cc257ba8`|
| `node safe-cex-ffi/scripts/setup.mjs` |0|0.21|`f9eedb9803429c8cbc38911d530cc5b7d2a083661f0ce5f007797f0e6a1173be`|
| `node safe-cex-transport/scripts/warning-wasm.mjs` |0|0.89|`403f3f59db17b96492d20b78bc5e17b06a13ceb44446fae1266d73f459666d8a`|
| `cargo test --locked -p safe-cex-venue-bitrue --all-targets --jobs 1 -- --include-ignored --test-threads=1` |0|235.42|`1dddf31755b39f140187b063d6833299f13fbfeb48e4dd6dca0ebba3e4963c26`|

The runtime command executes the transport warning fixture in actual wasm. This is not browser Worker execution or Bitrue-specific wasm runtime conformance. Policy clippy includes all targets of each wasm-classified crate and the declared FFI wasm feature; it is not a claim that native-only crates ran on wasm. The exchange default/all-feature builds are separate results.

## Additional symmetric wire control

Base `7b4b2980f98c0540b88f009ea6ff9c926c6bba0a`, temporary patch SHA256 `f94241f4a325fbcb32d1a35e68e81f29f4aea9e4d97f419c694729f066bbb58d`. Swapping only the high/low Serde companion names changes both serialization and deserialization; independent native `.high`/`.low` assertions remain unchanged. The mutated run compiled, ran one test and failed `native type parity: FETCH_KLINE_RESPONSE mode-0 public raw` with exit101. Restoring the exact original bytes passed with exit0. Mutation log SHA256 `413ac669c5f45ac5ce5eddfea67dd7a5ba74a0dc4c1a5d962570b8c8b6c2e1d8`; restored log SHA256 `c96d8f304c455792ba8aa10c79d515a7cf1c0b5204db38f755bcdfdb23394490`. The enumeration stops at iteration0, one `FetchKlineResponseItemWire` high/low pair. No survivor or compiler-blocked control occurred in this floor. It is separate from the nine historical controls at9f.

[Portable control runner](evidence/audit-remediation/wire-swap-control.py) requires Rust and the external TypeScript dependency on the supplied environment. Its generated patch/receipts belong under ignored `sdk-rs/target/bitrue-wire-control/`.

## Diagnostic provenance

Direct canonical check, report and progress each exited 1 with 5,323 diagnostics. The indexer completed. Replacing the revised annotations with the exact513-file integration-manifest snapshot produced identical multisets:0 added/0 removed. Compared with the historical5897 at9f,574 diagnostics disappeared and0 appeared. The common diagnostic body SHA256 is `9c498f8bfadf9d1d550feed14888dd48448293835077870dd0111d0b5906ccce`; enumeration stops at `safe-cex-webworker/src/worker_session.rs::WorkerTable`. Direct binaries retain their previously verified d4 build provenance; this is not a claim of rebuilding the checker for this measurement.

The historical+2 over5895 were introduced by source commit `a32f3998b1e348dbe7e0218d6962dde0ae1edc81`: `Exchange::recover_after_offline` and `WsProtocol::recovers_after_offline`. The first containing integration first-parent is `4891e46a0770116f292342ece37e7c6809983859` (PR7970), found after a five-commit ancestry floor. This branch imported it in merge `e98c0fd218a0d07afd4582b0a931acecb085a1ac`. Both diagnostics remain present once each; neither arose from these annotation changes.

At the recorded remote query, PR7957 was merged (574 removals); PR7967 and PR7972 were open. The4673 figure was reported on PR7972's continuation branch, which depended on PR7967, rather than on fetched integration49a. Its original baseline also omitted the offline pair. No unmerged continuation branch was imported. This historical provenance does not forecast a later integration revision.

## Remediation and comparison scope

Bitrue remains 82 ported /25 divergent /0 pending; this audit moves0 status rows. Its frozen25-row floor ends at `BitrueExchange.fetchBalance`:16 explanation findings received local rationales,6 scheduling findings received entry-timing fixes, and3 previously supported scopes were retained. Behaviour, documentation and already-known newly established finding counts are0. Explanation repairs do not establish behaviour parity. Direct maintained-TypeScript/native finite comparisons cover17 named executable rows;8 erased declarations have separate schema/decoder observations, not executable response-validator parity.

The historical manifest verification compares694e07f7e049a5a7047c41e994a0fc854dc78a59 to9f1be06793a067d64a079a9dbfebae84774cd01a. Integration subsequently changed targets on7 already-ported Bitrue rows; those incoming changes must not be attributed to this audit. Against measured integration49a, this deliverable changes25 notes and0 status/target/layer fields. The original25 divergent-row cohort itself is unchanged. Bitrue source/tests remain byte-identical to9f. All SDK Rust source, dependency manifests/lock and maintained Bitrue TypeScript are unchanged by the02a-to7b merge.

## Evidence handling and limitations

New raw gate stdout remains local and ignored under the merged repository policy; this authored SHA-bound attestation records measured exits and hashes. Earlier already-tracked evidence is historical and retained. No generated log is force-added. Controls are temporary patches over the stated base, with exact source restoration; compilation failures are not runtime discrimination.

**Development Cargo-overlap deviation:** the earlier Crypto.com sequence overlapped a Cargo-spawning CI-wiring import with a Cargo test, as disclosed on its merged PRs and this PR. This workspace queue executes one Cargo/Cargo-spawning operation at a time. No new overlap is claimed or hidden. The current sprint protocol forbids every push; delivery is a local commit for filesystem retrieval.

## Not run

The full native workspace test matrix, exchange host all-feature tests/lint, complete FFI/bindings checks, Linux numeric measurements and unrelated venue/shared source-oracle jobs were not executed in this unit; those are separately CI-enforced jobs. No remote CI run or rerun, full monorepo suite, browser Worker execution, exhaustive equivalence, autonomous Promise progress or cancellation proof is claimed. The requested workspace formatting/check/clippy/build and the complete configured wasm job commands were run; none belongs on the current NOT RUN list.
