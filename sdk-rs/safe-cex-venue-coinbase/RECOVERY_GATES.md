# Coinbase branch recovery gates

Recovered source branch `315e771e27e133ba74150ffef63b5294c3c1bdd9` was merged
with requested integration parent `36f5fca044b13fd4b674049b98941aa8632be8bf`
at `ef172ad257d5819ad12e58a517cef4dbf89813d5`.
Both sides of each Coinbase CLAUDE conflict were preserved. No CLAUDE or
agent-memory guidance was reread. Integration utility implementations were
retained; both the recovery and integration credential corpora remain tested.

## Census and real targets

Python 3.11 `tomllib` compares symbol keys in all 16 Coinbase manifests.
Stopped after iteration one, at the 426-row manifest floor. This is a census,
not proof of full venue runtime completeness.

| Revision | Ported | Divergent | Pending |
| --- | ---: | ---: | ---: |
| original merge-base `f297d9340b` | 258 | 13 | 155 |
| killed branch `315e771e27` | 281 | 15 | 130 |
| requested integration `36f5fca044` | 272 | 14 | 140 |
| recovered merged code | 279 | 17 | 130 |

True new gain versus integration: **10 pending → ported, 0 pending → divergent**.
The original 25-row count overlapped 15 rows integration already moved.
Three existing ported rows are reclassified divergent for witnessed UTF-8
representation limits: credential JSON parsing, string base64url encoding,
and normalization's downstream parser refusal. PEM remains divergent because
its unit error discards DOMException metadata. The recovered PEM padding
implementation also accepted `Zg=` unlike source atob; the integration decoder
preserves source rejection and avoids the recovery normalizer's duplicate-
footer detection defect. No unsupported PEM ported claim was retained.

[RECOVERY_ROWS.json](RECOVERY_ROWS.json) freezes the original 25 off-pending
identities and final mappings. Every snapshot rs/status was checked against
live TOML; `recovered_off_pending_rows_resolve_to_native_rust_items` parsed the
real Rust files with syn and resolved all 25 targets. Its explicit missing-
symbol control failed by name. This is target existence plus compilation,
not a claim that declaration existence proves every behavior.

## Gates and SHA binding

Measured at `e3f8f5cbcaf40976ecf9be549edb97789d853755`:

| Gate | Exit | Evidence |
| --- | ---: | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-coinbase --all-targets --jobs 2` | 0 | Finished, zero errors |
| scoped Coinbase `cargo test --all-targets --jobs 2 -- --include-ignored` | 0 | 175 passed, 0 failed/ignored across 35 completed test binaries |
| required Node/Yarn `parity:extract:check` | 0 | Output: 7341 symbols, 512 files, zero updates/additions/orphans |
| Coinbase Store reachability audit | 0 | Independent callable floor 159, matching declarations, only propagated notification paths |

Every oracle run sets
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
One Cargo was launched at a time. The extractor writer ran after manifest edits.
The first all-target run at merge `ef172ad257` failed (101) at
`utils: manual string-only decoder inventory`; the merged enum-based credential
carrier and recovered state methods required explicit classifications. Those
classifications now reject a deliberately introduced enum Deserialize derive
at a named assertion, rather than disabling the decoder inventory.

`bf946e6bec6de3c307a4549f95add51034c2e82a` adds only the tracked restoring
control driver; src/tests are byte-identical to the successful e3f8f5cbca gate
inputs. Controls below were measured at bf946e6bec with the mutation diff hashes
recorded in [RECOVERY_GATE_RESULTS.json](RECOVERY_GATE_RESULTS.json).
All changed files were restored byte for byte, and every restored target passed.

## Discriminating family controls

Stopped after these 13 controls. No mutation exhaustiveness is implied.
All mutant exits were 101 with the named assertion/test below; all restored
exits were 0. No listed mutation survived.

| Family | Named failure |
| --- | --- |
| snapshot | `snapshot batch count fixture` |
| ticker | `spot CFM decoded volume identity` |
| public-context | `native context success branch` |
| public-error | `public_test_error_matches_live_source_and_preserves_optional_data` |
| websocket-types | `websocket_session_literal_unions_match_closed_typescript_sets` |
| rpc-record | `absent ID is omitted while explicit-null error data is retained` |
| endpoint | `resolved endpoint` |
| crypto | `asn1Wrap` |
| credentials | `credential_normalization_matches_live_typescript` |
| decimal | `coinbase_decimal_guards_match_live_typescript` |
| private-health | `expected original Store notification failure` |
| decoder-classification | `utils::CoinbaseSignatureAlgorithm: derived/strict/serialize-only classification` |
| real-targets | `recovered target has no native Rust item` |

Controls mutate actual snapshot threshold, ticker fields, duplicate context
tracking, error safe-integer bound, decoded WS variant, RPC optional-ID omission,
endpoint URL, ASN.1 bytes, credential algorithm, decimal predicate, Store error
propagation, decoder exposure, and a real target mapping. Native strings,
variants, f64 bits, mapped fields and Store state are asserted before wire
serialization where applicable; the RPC declaration has no inbound decoder,
so its omission control tests its actual outbound contract.

Not run: workspace compilation, other venue tests, host Clippy, wasm Clippy,
full monorepo suites, or CI reruns. Source-level sub-agent observations were
review inputs, not claimed as executed gates. PR 7420 was merged by the user;
this gate sheet records the work actually measured, including the lower true
10-row gain and the classification corrections.
