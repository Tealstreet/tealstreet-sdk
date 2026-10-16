# Private-stream degradation and timer bodies

Moved **3 rows pending→ported, 0→divergent**: markPrivateStreamDegraded,
schedulePrivateStreamDegradedReconcile and schedulePrivateStreamWriteCheck.
Python 3.11 tomllib census exited **0** across eleven Bitget manifests and
227 independently expected, unchanged symbol identities: **46 ported / 80
divergent / 101 pending → 49 / 80 / 98**. This was measured against both
original integration parent `9f59912c6f39a0df543e6228c38ad5a57258d2a2` and merged
integration `5775b542a5f9f40174ec6d8c5d0d7902e46e49fb`.

Original tested code SHA: **`d56d763099c0e2c58987b326b5f10640ff4a4c15`**.
Its immediate parent is the original integration above; no separate unreviewed
lane branch was used as an ancestor. The port preserves the three actual field
bodies and two inline callbacks over supplied fallible receiver operations.
The host retains opaque values, UTF-16, timestamp aliases and original errors;
JS truthiness and ordered `>` coercion are supplied language operations.
Full exchange assembly, called SDK implementations and the REST reconciler
remain independently mapped; this does not claim their completion.

## Gates measured at the original code SHA

| Gate | Measured exit and observed result |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --all-targets --jobs 2` | **0**, finished dev profile |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --test stream_timers --jobs 2 -- --include-ignored` after restoring mutations | **0**, three passed, zero failed/ignored |
| Extractor writer after manifest editing | **0**, 7341 symbols; updated zero manifests, zero added/orphaned |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | **0**, 7341 symbols; would update zero manifests, zero added/orphaned |
| Scoped rustfmt check, `git diff --check` | **0**, **0** |
| Restored source/tests diff against tested code SHA | **0** |

Cargo ran sequentially with `/opt/homebrew/opt/rustup/bin` on PATH. Source execution
used `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
The extractor output text was inspected; exit status alone was not the criterion.
Manifest mappings/provenance used parseManifest/serializeManifest; twenty-two
actual Rust helper declarations/capabilities have specific exemptions.

## Native assertions and finite floor

The independent enumeration is **566 cases: 72 successful combinations and
494 injected failures**. It stops on write scheduling, old timer=true,
callbackDisposed=true, mode=nan, inline=true, failure index 7 (assignment after
inline callback). The oracle extracts the three maintained TS field initializers,
three source constants and actual Warning severity, and executes their bodies.
This is a floor, not complete venue or JavaScript input coverage.

Before any source output comparisons, native assertions check event prefixes,
decoded reason UTF-16/default/null, reason alias identity at health/log/recovery/
emitter call sites, forced warning options, timer slot/opaque token identities,
8000/10000ms delays, timestamp capture identity and ordered coercion failures,
write/launch counts, original failure identity, queued/inline callback execution
and state after failure. The mutable timestamp alias case observes a change to
the captured object; NaN and newer/equal timestamps take their source branches.
Degraded-reconcile callbacks still launch after both state flags change.
A separate native test drops the caller and verifies strong receiver retention
through callback execution, then release after the callback is dropped.
Native expectations and the 566 source count are independent of oracle outputs.

Each following production mutation compiled and failed the named native
assertion with exit **101**; restored target exit **0**, three tests passed.
No mutation survived. Working source was restored byte-for-byte.

| Row / deliberate mutation | Named assertion |
| --- | --- |
| markPrivateStreamDegraded: forced warning option true→false | `TIMERS native forced warning options` |
| schedulePrivateStreamDegradedReconcile: omit callback timeout unset | `TIMERS native event prefix` |
| schedulePrivateStreamWriteCheck: delay 8000→10000 | `TIMERS native decoded delay` |

## Integration merge and final validation

Merged integration `5775b542a5f9f40174ec6d8c5d0d7902e46e49fb` down as
**`df64a15226723f1dbfc7d9ae2701a32a9d1ce7cc`**. Merge exited **0** without
conflicts. Scoped source/tests and source-capture scripts diffs against the
original code SHA exited **0**: owned implementation/assertions are byte-unchanged.
Dependencies changed, so the scoped crate check and timer target were rerun.

At merged code SHA df64a15226723f1dbfc7d9ae2701a32a9d1ce7cc:
scoped cargo check **0**, timer target **0** (three passed, zero failed/ignored),
extractor check **0** (7341 symbols, would update zero manifests, zero added/
orphaned), tomllib census **0** (same three pending→ported changes).
The original compiling mutations were not repeated after merging because their
owned source/assertions are byte-unchanged. Evidence-only commits follow.

## Review citation and non-claims

Native sub-agent `/root/account_read_review` reviewed exact committed SHA
**d56d763099c0e2c58987b326b5f10640ff4a4c15** against its actual parent; reported
**no blockers** and supported ported classification within supplied-host scope.
It independently measured actual AST source capture **0** and Python3.11 checks
**0** for 566=72+494 cases, three mappings and twenty-two exemptions. It checked
default/alias preservation, timer order, callback capture/ordered coercion,
inline/deferred/failure paths and native-before-wire assertions. Reviewer did
**not** run Cargo, extractor, mutations, clippy, wasm or CI; those results above
were measured by the primary agent, not attributed to the reviewer.

Not run by primary: full venue test suite, workspace compilation, monorepo
suites, host clippy, wasm clippy/check, CI, invention checker, network exchange
integration or concrete scheduler binding. No completeness claim for the
supplied host, called SDK operations, REST implementation or entire exchange.
