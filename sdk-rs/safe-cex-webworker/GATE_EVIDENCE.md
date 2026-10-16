# Webworker gate evidence

This tracked record replaces the previously untracked attachment-only reports. Each table names the exact measured SHA; later documentation commits do not change those measurements. Cargo commands ran sequentially. No workspace compilation, full crate/monorepo suites, fresh host/wasm Clippy, or CI rerun is claimed.

## Original TWAP manager proof

Measured SHA: `f8631c8e7217f9541dc72940ae7a53f379d51a1c`.

Built on `2bc83d78d4f821c4b116d082d02c7b5a2595ffc5`.

Rows moved, measured with tomllib: 0 pending→ported, 7 pending→divergent.

| Gate/control | Exit | Required observed output |
| --- | ---: | --- |
| check | 0 | Finished |
| baseline | 0 | 1 passed |
| option-alias-control | 101 | TWAP-MANAGER native option alias value |
| restored | 0 | 1 passed |
| extract | 0 | Would update 0 manifests; 0 added, 0 orphaned symbols. |

Commands actually run:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --all-targets --jobs 2` (check).
- `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --test twap_manager --locked --jobs 2` (baseline).
- `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --test twap_manager --locked --jobs 2` (option-alias-control).
- `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --test twap_manager --locked --jobs 2` (restored).
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` (extract).

The compiling deep-copy-options mutation failed `TWAP-MANAGER native option alias value`; the original options value remained 44 rather than observing the required live value 99. Production bytes were restored, then the named test passed. Coverage floor: 15 fixed transitions, stopped at index 14; one mutation. Duplicate live IDs and options-root replacement remain unmeasured. The JSON shape comparison normalizes numeric values; it is not JSON-byte parity. The TWAP placement/timer lifecycle remains pending.

## Original margin repair proof

Measured SHA: `2c172364268eb9e674080a6d1aaa772cafe9faea`.

Branch based on integration `dc69e81e1153b0724bafc7977bbba877a8e128fc`.

Rows moved, measured with tomllib: 0 pending→ported, 1 pending→divergent.

| Gate/control | Exit | Required observed output |
| --- | ---: | --- |
| check | 0 | Finished |
| baseline | 0 | 5 passed |
| placeholder-order-control | 101 | MARGIN native fetched key order 31 |
| cleanup-order-control | 101 | MARGIN cleanup native fetched key order |
| old-drop-control | 101 | MARGIN old cancellation preserves restarted marker |
| restored | 0 | 5 passed |
| extract | 0 | Would update 0 manifests; 0 added, 0 orphaned symbols. |

Commands actually run:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --all-targets --jobs 2` (check).
- `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --test ui_margin --locked --jobs 2` (baseline).
- `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --test ui_margin --locked --jobs 2 margin_fetch_effects_match_live_source` (placeholder-order-control).
- `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --test ui_margin --locked --jobs 2 margin_cleanup_restores_native_numeric_key_order` (cleanup-order-control).
- `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --test ui_margin --locked --jobs 2 dropping_old_margin_fetch_does_not_clear_restarted_fetch_marker` (old-drop-control).
- `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --test ui_margin --locked --jobs 2` (restored).
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` (extract).

Placeholder and cleanup ordering controls each failed a direct native key comparison: actual OTHER/42 versus expected 42/OTHER. The old-drop control failed the named restarted-marker assertion. Production bytes were restored and all five tests passed. At `18b5fe3dff`, cleanup-only normalization removal had survived all four earlier tests (exit 0), a finding at `clear_fetched_leverage`; the separate native cleanup witness closes that measured gap. Control floor: three controls at the final margin SHA.

## Integration refresh

Measured SHA: `f3b532022493e832fa1f6315b76827fdd76be43e`.

Integration parent: `5ffc94fa620b26e96d54b08104a547ec76408dea`.

| Gate/control | Exit | Required observed output |
| --- | ---: | --- |
| check | 0 | Finished |
| focused-tests | 0 | 5 passed |
| extract | 0 | Would update 0 manifests; 0 added, 0 orphaned symbols. |

Commands actually run:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --all-targets --jobs 2` (check).
- `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --test twap_manager --test ui_margin --locked --jobs 2` (focused-tests).
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` (extract).

The merge retains both the TWAP and margin CLAUDE paragraphs. Scope comparisons against `f8631c8e72`:

- `git diff --exit-code f8631c8e72 -- sdk-rs/safe-cex-webworker/src/twap_manager.rs sdk-rs/safe-cex-webworker/src/lib.rs sdk-rs/safe-cex-webworker/tests/twap_manager.rs sdk-rs/safe-cex-webworker/tests/twap_manager_oracle.mjs sdk-rs/parity/manifest/webworker/twap/TwapManager.ts.toml`: **exit 0**.
- `git diff --exit-code f8631c8e72 -- sdk-rs/safe-cex-webworker/src`: **exit 1**. The sole source difference is the integration margin repair in `src/ui_state.rs`; the whole crate is not byte-unchanged.

The integration update also changes Cargo.lock and adds margin tests/oracles. Fresh scoped compilation and both focused targets passed: one TWAP manager test and five margin tests. TypeScript resolved through `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`. Extractor output was read: zero manifest updates, zero added and zero orphaned symbols. Existing controls were not rerun because their own production/test/oracle bytes are unchanged; their exits remain attached to the historical SHAs above.

Read-only sub-agent reviews of the original manager and margin slices found no blocker after the disclosed cleanup-witness repair. No separate sub-agent review of this documentation-only evidence recording is claimed.

The evidence-recording commit `cd80d52f5e` changed only this Markdown file.
`git diff --exit-code f3b5320224 cd80d52f5e -- . :(exclude)sdk-rs/safe-cex-webworker/GATE_EVIDENCE.md` exited 0: all other tracked gate inputs were byte-unchanged.
