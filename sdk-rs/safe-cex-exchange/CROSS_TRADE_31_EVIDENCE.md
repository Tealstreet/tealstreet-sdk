# Cross-trade 31-row evidence

Measured implementation/test/control SHA: `76429d632b9a3c8bd4ec0efbdfc724954d0d4390`. Actual integration parent: `c5842fe5e72e2d3eb2a734a5c2ad1c82b92bab6e`.
Branch: `feat/rust-core-cross-trade-31`; no unrelated unreviewed feature branch is an ancestor.
This evidence commit adds only these two evidence files; measured code/test inputs remain unchanged.

## Rows moved

Python 3.11 tomllib, crossTrade subtree, enumeration iteration 1: six manifests / 31 rows.
Before: 31 pending. After: **5 ported, 26 CHOSEN divergent, 0 pending**. No existing rows were demoted.
The ported rows are the two exact string unions, literal failure key, success-state deletion and min-amount predicate.
Every divergent row names its actual native target and its specific ownership, callback, prototype or scheduling boundary.
Native modules implement both symbol helpers, weight calculation, defaults/types, callable execution factory and the full
manager: eager audits, copier-specific chaining, failure mute state, retained in-flight completions and stop/restart.
The shared order constructor is exposed so the actual canonical builder can be used without another local implementation.

## Measured gates

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-exchange --all-targets --jobs 2`: **0** at `76429d632b9a3c8bd4ec0efbdfc724954d0d4390`.
- Scoped `cargo test ... -p safe-cex-exchange --test cross_trade --jobs 2 --message-format=json`: **0**, six tests passed at the same SHA. This tests the actual public library.
- Actual-source `tests/cross-trade-source.cjs`: **0** at the same SHA.
- Scoped tomllib census: **0** at the same SHA.
- Extractor `parity:extract:check`: **0** at `f513fb9ec6c971e948e928e1644e47488e76849b`; output read explicitly: `Would update 0 manifests; 0 added, 0 orphaned symbols.` Its complete manifest/extractor/TS-source/package-command inputs are byte-unchanged through the measured SHA; preservation diff **0** is recorded in JSON.
- `scripts/cross-trade-controls.py`: **0** at `76429d632b9a3c8bd4ec0efbdfc724954d0d4390`. Exact production module bodies are included directly in a Rustc test harness, using the measured Cargo runtime/types/num/Serde/JSON libraries and canonical order constructor. Baseline compile **0**, six native tests **0**. This bounded control harness is distinct from Cargo testing.

## Compiled discrimination

| Mutation | Named native assertion | Broken compile | Broken test | Restored compile | Restored test |
| --- | --- | ---: | ---: | ---: | ---: |
| weight-mode | `CROSS native weight-mode variants` | 0 | 101 | 0 | 0 |
| execution-mode | `CROSS native execution-mode variants` | 0 | 101 | 0 | 0 |
| literal-key | `CROSS native literal failure key` | 0 | 101 | 0 | 0 |
| clear-success | `CROSS native failure entry presence` | 0 | 101 | 0 | 0 |
| minimum-absolute | `CROSS native minimum amount predicate` | 0 | 101 | 0 | 0 |
| chase-dedup | `CROSS native eager execution events` | 0 | 101 | 0 | 0 |
| resize-threshold | `CROSS native eager audit calls` | 0 | 101 | 0 | 0 |

Every restoration is byte-exact; no survivors in these seven controls. Iteration 1 stops at seven; this is not exhaustive.
Native enum variants, order fields/prices/flags, event order, error metadata allocation and opaque result pointer identity,
audit quantities/references, failure entries/deadlines and loop state are asserted directly before any wire assertions.
No Rust decode/re-encode comparison is used. Source expectations execute the actual modules with actual lodash,
symbolConverter, currency, sizing and safe-math dependencies, alongside explicit independent literals/case floors.
Floors: 12 mappings, 19 weights, 22 execution cases, 22 audit cases, eight mute steps, lifecycle, pending-copier and
reentrant-stop witnesses. Source state DTOs copy primitive values, avoiding accidental alias collapse before serialization.

## Review and boundaries

Read-only sub-agent review at `76429d632b9a3c8bd4ec0efbdfc724954d0d4390` resolved the reentrant stop borrow, existing-entry visibility and success-state
proof gaps. The reviewer ran no Cargo or mutation controls; this lane measured the exits above.
The pending-copier witness proves that copier 2 proceeds to master audit while copier 1 waits; stop suppresses copier 1's
master audit and rescheduling. The source deliberately ignores config.enabled; native behavior preserves that quirk.
Chase terminates a different-size sole chaser before the zero guard. Warning failure replaces the order error; otherwise
the original native error allocation and opaque result identity survive. Success removes its target only and resets the
next failure to one. Stop clears state, but in-flight completions can repopulate it, preserving source behavior.

Source prototype findings are not hidden: ordinary forward maps may return an inherited constructor function; lodash
invert assignment to __proto__ leaves Object.prototype as the reverse lookup. Native own-string results intentionally
differ, and both helper notes are CHOSEN. Owned snapshots/defaults, readonly/frozen object aliases, weak loop lifetime,
Promise sibling cancellation, callback receiver ABI and typed logger/error/global capabilities are similarly named.
Concrete exchange/store/scheduler assembly and live/facade behavior are not claimed. Retained query decoding is untouched.

Initial isolated-harness compilation failed with incompatible host-build Serde artifacts before any mutation ran.
This is not a negative witness. The corrected harness proves the native ABI by compilation before its baseline and controls;
the exact selected artifact hashes, commands, exits and prototype failure are retained in JSON.
Not run: workspace/full monorepo builds, other crate checks, host/wasm clippy, wasm compilation, CI or live network tests.

## Merge-down preservation

Merged integration `dc74077e6165f04841de6a6bbd9d73c55afc58db`, producing `e399c5b03cd675589bbc6364ebe6e6db85680dcb`.
The preservation diff against `76429d632b` exited **0** for exchange source/tests, crossTrade manifests,
all local dependency crate inputs and source-oracle inputs. Measured implementation and assertions are byte-unchanged.
Cargo.lock gained only a safe-cex-candle-cache dependency on unrelated safe-cex-venue-apexomni.
A tomllib walk of the safe-cex-exchange transitive lock graph compared equal (exit **0**); no relevant package
version or dependency entry changed. Original Cargo/control receipts remain bound to the original measured SHA.
The merged-tree extractor exited **0** at `e399c5b03cd675589bbc6364ebe6e6db85680dcb`, with explicit zero-drift output.
Post-merge census remains 5 pending → ported, 26 pending → divergent, zero pending.
The following documentation commit changes only these two evidence files.
