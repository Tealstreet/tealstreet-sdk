# Concrete exchange class closure

Original integration parent: `ca0ce4fb921387e55a180c9bf136b88950cd8869`.
Latest required integration merge: `7bac54167c`, merged at `051b4af207559cc6bc068711fb9135a870d19bd7`.
Measured merged source/documentation commit: `343038a4b724a1d353eff73187c160ac18128c86`. No push.
Machine records and mutation patches: [gate-evidence/class-closure/](gate-evidence/class-closure/).

The parent and merged tomllib censuses are **268 ported / 18 divergent / 0 pending**.
**Three class FINDING rows closed; zero status transitions and zero pending rows moved.**
`census.json` compares identities and statuses to the actual parent; this is a
row census, not a semantic proof of all 286 symbols.

| Existing class row | Finding classification and repair | Remaining classification |
| --- | --- | --- |
| BinanceExchange | Behaviour: missing SDK trait dispatch, retained owner cycles, omitted imported response/header/rejection access | CHOSEN rationale at the canonical anchors below |
| BinancePMExchange | Behaviour: missing SDK trait dispatch and retained owner cycles | CHOSEN rationale at the canonical anchors below |
| BinanceSpotExchange | Behaviour: missing SDK trait dispatch and retained owner cycles | CHOSEN rationale at the canonical anchors below |

Concrete class composition also now preserves Store subscriber metadata, listener
setup/cleanup errors and worker after-hook rejection. Worker unsubscribe keeps
the maintained catch/delete ordering. Supported routes and real capability brands
are bound through the shared Exchange trait; source inherited defaults remain inherited.

Residual rationale is declared only at the
[snapshot anchor](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias) and
[native lease anchor](../parity/MANIFEST-SPEC.md#owned-exchange-lease-policy),
with the [classification taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy).
No declaration is duplicated in row notes. The classes remain divergent.

`merge-provenance.json` records measured diff exits 0 from `bfca278766` to the
merged commit for Binance production, tests/oracles, manifest rows and worker
control sites. Only exchange.rs has the described additive shared-operation
change; other shared-core and integration changes are not claimed unchanged.
Premerge gates retain their original SHAs in `pre-merge-bfca278766/`.
The four required gates and six affected Cargo test scopes were rerun against
the merged tree. Generator/extractor records retain their original measured SHA;
they were not rerun after the merge.

| Required handoff gate | Measured SHA | Exit | Seconds |
| --- | --- | --- | --- |
| workspace-fmt | `343038a4b724a1d353eff73187c160ac18128c86` | **0** | 11.96 |
| workspace-build | `343038a4b724a1d353eff73187c160ac18128c86` | **0** | 57.88 |
| workspace-host-clippy | `343038a4b724a1d353eff73187c160ac18128c86` | **0** | 234.7 |
| workspace-wasm-clippy | `343038a4b724a1d353eff73187c160ac18128c86` | **0** | 274.83 |

Commands: cargo fmt --manifest-path sdk-rs/Cargo.toml --all --check;
cargo build --manifest-path sdk-rs/Cargo.toml --workspace --locked --jobs 2 --keep-going;
cargo clippy --manifest-path sdk-rs/Cargo.toml --workspace --all-targets --locked --jobs 2 --keep-going;
node .github/scripts/rust-wasm.mjs --clippy. Exact argv is recorded in each JSON.
Toolchain 1.99.0, incremental 0, one Cargo at a
time. The wasm helper consumes the canonical matrix; host and wasm are separate measurements.

| Scoped gate | Measured SHA | Exit | Passed / failed / ignored |
| --- | --- | --- | --- |
| binance-types | `bfca278766dea2b4ee5a2aa977137f6833915a09` | **0** | 0 / 0 / 0 |
| binance-all-targets | `343038a4b724a1d353eff73187c160ac18128c86` | **0** | 298 / 0 / 0 |
| core-library | `343038a4b724a1d353eff73187c160ac18128c86` | **0** | 106 / 0 / 4 |
| core-lifecycle | `343038a4b724a1d353eff73187c160ac18128c86` | **0** | 15 / 0 / 1 |
| worker-subscriptions | `343038a4b724a1d353eff73187c160ac18128c86` | **0** | 7 / 0 / 0 |
| backpack-adapter | `343038a4b724a1d353eff73187c160ac18128c86` | **0** | 5 / 0 / 0 |
| bitget-subscriptions | `343038a4b724a1d353eff73187c160ac18128c86` | **0** | 2 / 0 / 0 |
| extractor-writer | `bfca278766dea2b4ee5a2aa977137f6833915a09` | **0** | 0 / 0 / 0 |

The canonical Binance run includes ignored Node oracles and stops at tests/types.rs;
its 298 passing tests span 73 reported targets, not 298 independently audited symbols.
Zero Rust counts on generator/extractor rows mean they run no Rust tests.
The Yarn writer output explicitly reports Updated 0 manifests; 0 added, 0 orphaned
symbols. Its text was inspected; no Volta shim exit is substituted for that check.
No manifest regeneration was used.

The independent quiescent regression at `f926279afb4a5068e62232eb3c13d61454f41ac1`
exited **101** at `native quiescent class state releases: binance, dispose=false`;
the repaired run at `e92981659384cce263d42a7aa5e77c6db9cdc746` exited **0**.
A shortened exact-name invocation initially selected zero tests; that exit 0 is
excluded from the evidence. Constructor retention tests were also corrected to
release two explicit parent Rc views before testing final reclamation. A pending
request fixture now really suspends rather than returning Ready immediately.
These fixture corrections are not the production-cycle evidence.

The constructor-watchdog regression at `6487bef24d` exited **101** at
`native constructor health sync schedules one watchdog: Futures, retained_child=false`
(observed 0, independently expected 1). Binding the stable core repaired it. The
first restored witness incorrectly expected an uninitialized socket to publish
false and failed 101 at `fdbded7a1b`; the corrected witness forces false before
checking the actual source's true publication. At `bfca278766` it passed 0.

Native enumeration floors: six quiescent combinations stop at Spot/dispose=true
(case 5); retained-child, pending-job, construction-failure and isolated-view cases
stop at Spot (case 2). SDK dispatch covers three concrete variants with native
request routes, 7.25 number bits, capabilities, optional history 37 and unsupported
inherited defaults. External listener setup/cleanup and worker confirmation test
original error metadata and commit-before-notify. The seven-case getter oracle
stops at the retained external interceptor after the local owner binding clears
(case 6). Native value/error assertions precede actual-source comparison.
The six constructor-watchdog combinations stop at Spot/retained-child=true
(case 5), checking 10000ms delay, independently expected one audit, real publication,
one successor, disposal and final weak reclamation. Retained-child source sibling
reachability and retained-interceptor witnesses describe observable call results;
they do not establish JavaScript garbage collection behaviour.

Eleven controls ran at `bfca278766dea2b4ee5a2aa977137f6833915a09`.
Each mutation exited **101 at a named runtime assertion**, then the restored
single test ran and exited **0**. `controls.json` records exact commands, mutation
patch hashes and assertion names; the ten Rust patches are retained beside it.
The source control uses the oracle's in-memory maintained-source rewrite, with no
production TypeScript edit. All eleven controls completed; no survivor is omitted.
`final-restore.json` records the final `git diff --exit-code HEAD` exit 0.

| Control | Named assertion |
| --- | --- |
| owned socket registration | `native quiescent class state releases: binance, dispose=false` |
| strong imported observer | `native quiescent class state releases: binance, dispose=false` |
| SDK Futures route | `native SDK fetchOrders request family: Futures` |
| native callable header | `native imported header getter selects 2.5 rather than own 9` |
| rejected-response accessor | `native rejected response uses live callable headers` |
| listener cleanup error | `native SDK depth cleanup returns provider error` |
| Store error conversion | `native SDK worker confirmation preserves subscriber metadata` |
| worker after-hook | `native successful worker reply preserves concrete after-hook rejection metadata` |
| worker cleanup/cancel order | `native cleanup failure skips throttle cancellation: book=true` |
| source callable header | `actual callable header chooses 2.5 rather than own 9` |
| constructor watchdog binding | `native constructor health sync schedules one watchdog: Futures, retained_child=false` |

The callable-header native witness distinguishes pause 22500 from 29000, selecting
2.5 instead of the own header 9. The source corruption is caught only after all
native assertions have run. Counts come from authored case/slot expectations,
not the length of the result being checked. No whole-venue or arbitrary
executable-object/lifetime equivalence is claimed.

Earlier unsuccessful implementation/test steps are retained as failures rather
than promoted to evidence: `9f1f609c76` check 101 (duplicate optional argument helper
and absent Serialize bound), `1e32183c85` check 101 (private fixture helper access),
`76b15f3099` test 101 (unused fixture import), `27cf2d5b4d` private tests 101
(16 passed / 3 failed; Store metadata plus the two authored lifetime mistakes above),
`e273d4cf90` compile 101 (missing cancellation conversion), `69cd2acb25` worker
subscriptions101 (wrong authored pending-timer count). Their fixes precede the
recorded passing measurements. The old pipeline at `960398802c` was intentionally
stopped 75 before its next gate for the constructor watchdog repair;75 is not a
failed Cargo invocation. Its initial ten killed/restored controls and297passing
Binance tests are archived under `initial-960398802c/`, not relabeled as new runs.

Extra static checks, outside the four required handoff gates, have actual failures:

- Core `override_contract` at `bfca278766` exited **101**:3 passed / 1 failed / 1 ignored.
  Enumeration stopped at its first diagnostic,
  `safe-cex-venue-apexomni/src/exchange.rs::ApexOmniExchange::dispose`,
  `dispose bypasses dynamic hook dispose through base`. The unchanged compiled
  scanner and unchanged override inventory also failed 101 against the verified
  `ca0ce4fb921387e55a180c9bf136b88950cd8869` runtime source corpus, then source
  restoration diff exited 0 (`parent-override-control.json`). This was an AST
  scanner invocation against parent source, **not a Cargo build at the parent**.
  No claim is made about later sites the first failure prevents enumerating.
- The built `parity-check` at `bfca278766` exited **1**, stopping before mapping
  validation at `safe-cex-num/src/prototype_realm.rs`: item macro `thread_local`
  cannot be indexed without expansion. Both that source and the checker were
  unchanged from `ca0ce4fb92` in that measurement. This is not a mapping pass or
  a parent-commit Cargo run. Rebuilt at `343038a4b7`, it still exited **1** at
  the same `thread_local` site (`parity-mapping-post-merge.json`). No gate was
  weakened to conceal either failure.

Not run: full workspace tests/doctests, separate wasm build, all-feature workspace
variants, and the remaining long-tail static/report suites. The four required
handoff gates are measured separately above; those exclusions do not imply an
unqualified CI-green or whole-venue parity claim. Existing ignored core tests are
reported as ignored, not passed. No pushes or CI reruns were performed.
