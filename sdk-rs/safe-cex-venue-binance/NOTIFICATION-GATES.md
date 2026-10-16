# Binance discarded reconnect notification repair

Implementation measured at `945f8a3b3c6862b18f1a8ab5ac9ad15f219af48b`.
Original integration parent: `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`.
The branch also contains earlier, unmerged PR 7851 work; these changes extend that PR.

The six private ping/open and five public open bodies now continue after a
rejected reconnect promise discarded by core `send`. Reconnect's synchronous
prefix still runs immediately. Its rejected continuation is reported after a
checkpoint through independently retained work, including after socket disposal.
Direct warning/send failures and explicitly requested reconnect failures retain
their synchronous behavior. Store notification conversion retains the original
opaque callback cause across two nested Store boundaries.

Retaining the original `Rc` callback error makes `ExchangeError` non-Send and
non-Sync. That public API cost is explicit; the existing local runtime model is
preserved, and no claim is made about compatibility with external Send bounds.

## Native evidence

Every native assertion runs before the actual-source comparison. The independently
authored 64-case floor consists of 24 private cases and 40 public cases, stopping
at case 63. Checks include partial health effects, skipped later subscribers,
local continuation, deferred report count, disposal survival and original cause
pointer. The source helper additionally exercises two real nested DefaultStores.
The typed health carrier asserts ten independently selected defined fields;
undefined timestamp/close-code fields are outside that JSON carrier. Numeric
normalization occurs only in the later wire comparison.

Caller-owned state and configured parent/order/subscription operations remain
body boundaries. This evidence does not claim full exchange-owner parity or
identity of an outer error wrapper. Classification follows the
[canonical taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy).

| Gate at the measured implementation SHA | Exit | Observed result |
| --- | ---: | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-venue-binance --all-targets --jobs 2` | 0 | All Binance targets checked |
| `cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-venue-binance --jobs 2 --test send_notification -- --include-ignored` | 0 | 11 passed, zero failed/ignored |
| `cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-exchange --jobs 2 --lib owned_tasks::tests::` | 0 | 16 passed, zero failed/ignored |
| `git diff --exit-code HEAD` after mutation restoration | 0 | No deliberate mutation active |

Extractor writer and check both completed with exit 0 after the eleven manifest
updates: `Updated 0 manifests` / `Would update 0 manifests`, zero added/orphaned.
The full `yarn venue-oracle:binance` gate completed with exit **0** at the
implementation SHA: **285 passed / 0 failed / 0 ignored** across 72 binaries.
Independent enumeration found 71 top-level integration targets plus the library;
it stopped at `tests/types.rs`. The generated-type check and Yarn completion
text both succeeded. Workspace `cargo fmt --all --check` also completed with exit **0**, measured
at evidence revision `8955ed46632a219004ecc4b58221cad39f33bc47` with unchanged
Rust/test/oracle inputs. Machine
records with measured SHAs and source-diff hashes are in
[gate-evidence/integration-gates](gate-evidence/integration-gates).

## Discriminating controls

There were **12 negative runs covering nine distinct mutations**, followed by
nine restore gates. All twelve negatives exited **101** at a named runtime
assertion; all nine restores exited **0**. Four native continuation families
share the same synchronous-propagation mutation and are counted once among the
nine mutations. This is a selected control floor, not an exhaustive fault census.

| Mutation/run family | Named failed assertion |
| --- | --- |
| Propagate the discarded reconnect rejection: private ping, private open, public open | `native venue operation continues after discarded reconnect rejection` |
| Same mutation: Spot private open | `native Spot open does not force close after background rejection` |
| Omit opaque callback cause | `native report retains original opaque cause pointer` |
| Replace original cause at a nested Store boundary | `native nested Store retains original cause` |
| Release pending detached owner | `native detached task owns its pending registry` |
| Retain detached owner after completion | `native completed driver releases detached lease before host drop` |
| Source corruption: skip reconnect | `send reaches health notification and skips later subscriber on throw` |
| Source corruption: skip reset | `public open resets orderbook despite background rejection` |
| Source corruption: skip private open continuation | `private open continues after discarded background rejection` |
| Source corruption: await Spot reconnect | `send reaches health notification and skips later subscriber on throw` |

The control records require a Rust panic site and failed-test result as well as
the named assertion. Compiler errors cannot satisfy these controls. No survivor
was dropped. All source bytes were restored before the final positive gates.

Earlier unsuccessful gates remain recorded at their actual SHAs: the run at
`cd1af89d480ca90389d8ec3c4c6b74e96efa7048` exited 101 on numeric JSON storage in
the wire comparison after native checks; the run at
`0f18b70eb14ea221f6cd1a7ae83cacd1cc5c3a87` exited 101 during compilation of a
test accessor. Neither is reported as a mutation kill. The corrected target at
`c582a6408b21fa5e91262b20c2c49d029ba3e4eb` passed 11 tests before the final local
subscription API cleanup and repeated positive gates at the measured SHA above.

## Remaining gates and findings

The workspace build/doctest and host/wasm lint results in
[INTEGRATION-GATES.md](INTEGRATION-GATES.md) remain attached to their earlier
SHAs. They do not cover this source change. The earlier wasm keep-going run was
interrupted and is not a pass. Both full lint lanes, both build lanes and
doctests must complete against the final repaired source. CI was not rerun;
live venue calls and browser execution were not run. Whole-venue parity remains
unclaimed while prototype/statistics, historical typed-output and owner findings
remain open.

## Local review

Two independent read-only sub-agent reviews approved these eleven method-body
promotions after correcting the notes: exact Rust targets are retained, the
64-case source-comparison target is named separately from the native-only family
tests, and Spot's attempt-counter saturation witness remains inventoried. The
reviewers ran no tests or mutations. The resulting measured venue census is
263 ported / 23 divergent / 0 pending, across twenty files through
`binancespot.ws-public.ts.toml`. Exactly eleven divergent rows moved to ported.

The full Binance suite ran at the implementation SHA above while intervening
commits changed only manifest/evidence inputs. The recorded source-preservation
comparison exited 0. Its results remain attached to the measured SHA.
