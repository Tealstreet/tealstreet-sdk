# Shared core divergence audit measurements

Frozen parent: `f853d56fd227c2106dba609141b1fdd44bcab125`. The new audit branch
starts directly there, without the two open venue-audit feature branches as
ancestors. Python 3.14 tomllib measured 151 direct exchange-level and 53 direct
utility divergent rows; pending is zero. The requested 139/49 counts are lower enumeration floors, not this cohort's
denominator. Four exchange annotations and
one utility annotation already mention FINDING; new findings are counted apart.

## Monthly calendar source and native observations

Measured clean commit: `d41403c0bd41ad3c0381cacf82173b20c1ab6248`.
With SAFE_CEX_TYPESCRIPT_PATH supplied, from the repository root:

`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-candle-cache --test divergence_audit narrowed_calendar_observed_findings --locked --jobs 2 -- --ignored --exact`

Baseline exit **0**, one named test, 11 independently expected identities.
Native returned-number bits are asserted before launching the unchanged-source
oracle. The source oracle checks typeof and exact numeric/NaN values before
encoding bits. Seven contrasts and four matching epoch controls are observed:

| Case | Actual source | Actual native |
| --- | ---: | ---: |
| Monthly start at year 1 January 1, epoch seconds -62135596800 | -2177452800 | -62135596800 |
| Monthly next at the same input | -2174774400 | -62132918400 |
| Monthly relative close at the same input | -2174774400 | -62132918400 |
| Monthly inclusive count from year 1 to year 100 January 1 | -21611 | 1189 |
| Monthly start at NaN | NaN | 0 |
| Monthly next at NaN | NaN | 2678400 |
| Monthly count from NaN to zero | NaN | 1 |

The four matching epoch controls are start 0, next 2678400, close 2678400 and
count 2. The typed interval is the valid 1M variant and finite year inputs are
already seconds. These differences are separate from the old note's explicit
alias/unit narrowing; no invalid enum or millisecond witness is substituted.

A temporary production control at the same commit changed only monthly start's
`days(y, m, 1) as f64 * 86400.` to that expression plus 86400. The exact command
exited **101**, with named assertion `CORE-CALENDAR native value start-year-one`.
A finally block restored the original file bytes. `git diff --exit-code` on
calendar.rs returned **0**, and the restored named test exited **0**. The control
had no survivor. No maintained TypeScript source was changed.

These are recorded current discrepancies, not a runtime repair. The test pins
the actual native results and separately observes source results; it does not
claim the two runtimes agree on the seven contrasts. The scope ends after the
11 named cases, not all calendar inputs.

No workspace build, workspace host clippy, wasm compilation/clippy/runtime, full
suite, live network, Node 26/other architecture or CI rerun accompanied this
new calendar measurement. The earlier requested workspace host clippy exit 0
remains attached to the separate combined Aster-repair verification commit
669e0246b5d3a4eb8fc877901b8d54c1e429e8ba in the
[prior PR 7920 evidence](https://github.com/Tealstreet/tealstreet-next/blob/e870e8ffd03554337ffbda6576f1b94b0701aaea/sdk-rs/parity/DIVERGENCE_BLOCK_GATE_EXITS.md).
That separate branch is not an ancestor here; this is not a fresh compilation.

## Gateway unknown-object capture observation

Clean measured commit: `d1f3633e630a3a532823c4e3ac2c07baafda2b75`.

`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-registry --test public_data_providers gateway_number_tag_object_observed_finding --locked --jobs 2 -- --exact`

Baseline exit **0**, one named test, five existing tests filtered out. A literal
raw JSON frame with a complete valid ticker and info object
`{"$serde_json::private::Number":"17"}` passes through the actual provider's
registered GatewayListener and parent update. Direct native assertions observe
one ticker, sibling bid 101 and info as Number 17. No native value is re-encoded
to establish its shape. The separately executed whole TypeScript provider probe
observes an object with that own key and string value at the same parent update.
This pins a current discrepancy, not a successful parity guard or a repair.

A temporary production control cleared ticker.info in handle_tickers before
parent delivery. The command exited **101**, named failure
`CORE-GATEWAY native info presence`; exact original production bytes were then
restored by the control driver's finally block. A restoration command was
started prematurely while the mutation still existed and overlapped the first
Cargo, violating the one-Cargo rule. It also exited **101** at the same assertion
and is not a restored pass or a second distinct control. Both commands finished;
the subsequent byte-identity check exited **0** and the serial restored command
exited **0**. No terminating signal was delivered: an attempted cleanup found
all three captured processes had already exited. This workflow error supplies
no extra proof. The one distinct control had no survivor.

## Scoped compilation

| Measured clean SHA | Command from repository root | Exit |
| --- | --- | ---: |
| 0216e02521c06afaab29fd575649498e810a66ac | `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-candle-cache --all-targets --jobs 2` | 0 |
| d1f3633e630a3a532823c4e3ac2c07baafda2b75 | `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-registry --all-targets --jobs 2` | 0 |

These are two separate scoped crate checks, not workspace compilation. The
calendar production source and gateway production source both match their Git
commits after the controls; neither runtime repair is included in this branch.

## Direct gateway declaration follow-up

At clean commit `5b4cd7574a33ed7185c26b6365e0948ed975b9a7`, the same named
gateway test additionally decodes the original literal frame directly through
GatewayMessage and asserts data[0].info as Number 17. Exit **0**, one named test
covering two distinct native entrypoints; this is not two tests. The production
provider result and direct declaration result are separate assertions. The earlier
d1f3633e control and its restored pass remain attributed to that earlier test
version; no second mutation of the later decoder assertion was run.

## Read-only source observations

All commands below supplied SAFE_CEX_TYPESCRIPT_PATH and used Node 24.20.0 on
arm64. Reviewers executed the maintained source initializers/modules with their
real SDK dependencies or explicitly bounded platform hosts. Source values,
identity, code units and effects were inspected before observation serialization.
These finite source observations are not native conformance gates.

| Scope | Measured SHA | Exit | Bounded observation |
| --- | --- | ---: | --- |
| `node sdk-rs/safe-cex-registry/tests/loaders.cjs` | bcdefd57916fec3c48de3d9269875e217c6a8215 | 0 | 94 entries: each actual literal module/export, constructor identity, missing export, error identity and source prefix timing. |
| `node sdk-rs/safe-cex-registry/tests/native_load.cjs` | d41403c0bd41ad3c0381cacf82173b20c1ab6248 | 0 | Actual reusable native load wrapper callback/constructor timing and synchronous failure. |
| Actual callback-type AST supplement | 0216e02521c06afaab29fd575649498e810a66ac | 0 | Two actual type declarations: variadic any[]/void handlers and the three declared statistics fields with optional markers. |
| Whole-provider source supplement, initial and expanded | 0216e02521c06afaab29fd575649498e810a66ac | 0 each | Independent floors 11 then 13, ending at income-public-dedupe-and-eager-entry. Real auth/subscription/lookup/update units, valid interval, retained ticker.info object, and eager pager/signer effects. |
| Utility source supplement, initial | d41403c0bd41ad3c0381cacf82173b20c1ab6248 | 0 | 14 fixed source cases, ending at returned parsed-object unit. |
| Utility source supplement, expanded | 0216e02521c06afaab29fd575649498e810a66ac | 0 | 16 fixed source cases, ending at actual risk symbol comparison yielding 20 versus 30. |

The three temporary supplements, row inventories and raw captures remain outside
the repository. Existing registry oracles and the new calendar oracle remain
executable. No new source-probe harness failures were reported. Read-only Git
input comparisons exited **0** for registry/type source and mapped native files
through 0216e02521, and for utility/provider source through that same commit.
Individual inspection independently matched all 94 native route literal pairs,
not merely their count. Native module hosts and most utility/provider paths were
code-inspected; only the named calendar/gateway tests above were newly executed.

## Final annotation census and extraction

At `306ed4e58edbd32bb71b028d82eb195e6fc41598`:

| Command | Exit | Result |
| --- | ---: | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-registry --all-targets --jobs 2` | 0 | Scoped registry check includes the direct declaration assertion. |
| `yarn parity:extract:check` through explicit Node/Yarn | 0 | Output read: 7,341 symbols, 512 files, would update 0, added 0, orphaned 0. |
| Python 3.14 tomllib ordered-row comparison against frozen integration parent | 0 | Exchange-level 206/151/0 plus one n-a, utilities 153/53/0; 126/43 new notes, four/one prior finding annotations retained; zero status/target/identity changes. |
| `git diff --exit-code` against integration over maintained TS and the two production Rust source directories | 0 | No TypeScript or Rust production source changes; this is equality evidence, not compilation. |

An initial census invocation at the same SHA exited **1** because its prior-note
counter included a ported row saying `resolved Rust FINDING`. Restricting that
counter to the frozen divergent cohort gives the **0** result above. The resolved
ported note was not changed or counted as open work. The canonical writer after
the 71 additional note edits exited **0**, updated/added/orphaned 0. The earlier
98 registry-note edits likewise used the writer, exit **0**, zero updates.

The author-reviewed outcomes reconcile as **28 supported / 169 new findings /
5 already-recorded finding annotations / 2 unresolved** across all 204 frozen
identities. The 169 new findings comprise 158 justification, four scheduling and
seven behavior findings. Of the justification findings, 119 specifically lack
a local choice rationale: 96 registry entries/objects, twenty utilities and
three socket notes. They are not newly measured runtime route failures.
Status rows moved: **0**. Final iterations: exchange-level **151**, utilities **53**.

Cross-review challenged twenty initial utility supports and three core supports;
those became annotation-only findings under the same rationale criterion already
applied to registry notes. Four whole-container losses were clarified. Existing
finding-bearing annotations were separated rather than credited as new findings
or clean CHOSEN explanations. Two redaction rows remain unchanged/unresolved;
private malformed-key and fixed-table-surrogate subclaims remain unproved parts
of separate annotation findings. No source private-map/prototype mutation was
invented to make a domain witness succeed.

No workspace build/clippy, wasm compilation/clippy/runtime, full suites, Node 26,
other architectures, live network, new deployment or CI rerun was run for this
shared-core audit. The earlier workspace host clippy exit 0 is a separate
measurement at 669e0246b5d3a4eb8fc877901b8d54c1e429e8ba, not a fresh gate here.

## Integration merge-down and post-merge metadata checks

The branch was frozen and the named native fixtures/checks were built on
f853d56fd227c2106dba609141b1fdd44bcab125. Before publishing, the branch merged
integration d74a1c01f77439f9df61fa330063be194aab7576, producing
`7c2207e21c6540d49e45e458937c296154d32864` with first parent
4dd2684615c671879a39236f3302795ed0f5253c. This brings the integrated CI-oracle
dependency recovery; no open venue-audit feature branch was used as a parent.

At the clean merged SHA 7c2207e21c6540d49e45e458937c296154d32864:

| Check | Exit | Result |
| --- | ---: | --- |
| Extractor check through explicit Node/Yarn | 0 | Output read: 7,341 symbols, 512 files, would update/added/orphaned 0. |
| Tomllib ordered-row comparison against frozen parent | 0 | Same triples, 126/43 new notes, four/one prior annotations, zero status/target/identity moves, original suffixes retained, both unresolved rows unchanged. |
| Git input comparison against 306ed4e5 | 0 | All Rust production src, Cargo manifests/lock, maintained TS, both new native fixture targets, calendar oracle and accounts-source loader are byte-identical. |
| Added SDK path scan against merged integration | 0 | No laptop path in authored SDK additions. |
| `git diff --check` against merged integration | 0 | No whitespace errors. |

The merge changes the CI helper but does not change those named native compile
and fixture inputs. This is input equality, not a fresh compilation, native
fixture rerun, workspace gate or integration CI result. Earlier native and
compile exits remain attached to their actual pre-merge SHAs above. The feature
is pushed for Sam's merge; this agent never merges it into integration.

## Per-class count follow-up

At `7b34c8abf45f93f6810a634c640eb306d5424cfa`, read-only tomllib scans of
both ac0f5fccf2 and f853d56fd227c2106dba609141b1fdd44bcab125 exited **0**:
exchange-level 206/151/0 plus one n-a; direct utilities 153/53/0. Neither scan
reproduced the requested 139/49 divergent denominators.

The class/file/kind census exited **0**, reconciling all 204 frozen identities
with the committed per-row decisions and note prefixes/suffixes. Expected totals
were fixed separately at exchange 151/21/126/4/0 and utilities 53/7/43/1/2
(audited/supported/new/prior/unresolved). New finding classes are exchange
122 justification, two scheduling and two behavior; utilities 36 justification,
two scheduling and five behavior. The owner and file tables each sum to their
independent frozen group floor. A draft read-only census invocation exited **1**
for a Python tuple-parenthesis SyntaxError before processing data; the corrected
invocation supplies the **0** result above. No new native/Node oracle, mutation,
Cargo, extractor or CI run accompanied these count-only observations.
