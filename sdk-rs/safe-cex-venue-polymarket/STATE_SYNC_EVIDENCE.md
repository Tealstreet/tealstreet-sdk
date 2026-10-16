# Polymarket account-state/lifecycle evidence

Moved **8 pending -> ported, 0 -> divergent**: credential derivation, startup,
tick, balance, positions, position commit, private refresh and trade update.
Tomllib census: **91 / 18 / 32 -> 99 / 18 / 24** (ported/divergent/pending).
The independent inventory is 141 rows in seven manifests, stopped after
polymarket.ws-public.ts.toml; this is a symbol census, not proof of whole-venue
runtime completeness. Exact rows/targets are in post-merge-gates/census.json.

Original integration parent: `970c8a3a41e3ff7e0da31cbf524f7d6b0e064849`. Implementation commit:
`200d205d8780f0fabd47898eaf774e54f3f3d08e`. Native expectation correction: `7a4c22423d`.
Merged integration `f364cee096ff92584c157cad8b96f12b92cf3a09` down into
`f6c43d3bf7c6d3bbdd4e79118e79b27abb48568d`. No foreign unpublished lane branch was used as an ancestor;
foreign work arrives through integration. Delivery branch:
`feat/polymarket-market-sync-970c8a`.

## Measured gates and their SHAs

Initial seven gates at `200d205d8780f0fabd47898eaf774e54f3f3d08e`; post-merge seven gates at
`f6c43d3bf7c6d3bbdd4e79118e79b27abb48568d`. Each receipt includes the exact command, exit and SHA.

| Gate | Initial exit | Post-merge exit |
| --- | --- | --- |
| source-oracle | 0 | 0 |
| check | 0 | 0 |
| tests | 0 | 0 |
| fmt | 0 | 0 |
| targets | 0 | 0 |
| writer | 0 | 0 |
| extractor | 0 | 0 |

Initial venue test floor: **111 passed** (previous independent expectation103
plus eight new tests); stopped after ws_timer_clear and zero doc-tests.
Post-merge tests intentionally cover **eight state_sync tests** after the test
correction; the full111 tests were not repeated at that SHA. All production
source bytes and other Rust tests/oracles stayed unchanged; the two test-file
changes, foreign lock dependency and manifest-specification edits are disclosed
in post-merge-gates/initial-preservation.json (32 input paths, strict differing
paths retained). Fresh check covers all targets of this crate, with jobs2 and
locked dependency resolution; this is not workspace compilation.

Both source-oracle runs use the actual eight TypeScript bodies and imported
source enums/maps/id helper,80 independently expected fixtures (eight x ten),
stopped at handleTradeUpdate/9. Signing, current child calls, transport and loop
are bound dependencies. Both target gates use the real venue-scoped indexer,
empty exemptions and141 rows/320 public items (289 prior + eight methods +
StateHost and22 members). Both writer/check outputs explicitly report zero
updates/additions/orphans; exits alone are not used as extractor proof.

## Discrimination

At `f6c43d3bf7c6d3bbdd4e79118e79b27abb48568d`, baseline eight tests exit0. Each row below compiled,
ran the named exact test, failed a decoded native assertion with exit101,
restored the original source in finally, and passed with exit0. Eight controls
executed, stopped after handleTradeUpdate. **No survivors or wrong failures.**
No control matrix for previously ported rows was rerun.

| New source family | Named test | Native assertion | Baseline / broken / restored |
| --- | --- | --- | --- |
| deriveApiCredentials | `derive_native` | native derived credentials original alias | 0 / 101 / 0 |
| start | `start_native` | native startup original markets alias | 0 / 101 / 0 |
| tick | `tick_native` | native bits tick.upnl | 0 / 101 / 0 |
| fetchBalance | `balance_native` | native bits balance.total | 0 / 101 / 0 |
| fetchPositions | `positions_native` | native mergeable | 0 / 101 / 0 |
| fetchAndSetPositions | `snapshot_native` | native snapshot committed original alias | 0 / 101 / 0 |
| refreshPositions | `refresh_native` | native refresh error text | 0 / 101 / 0 |
| handleTradeUpdate | `trade_native` | native bits fill.amount | 0 / 101 / 0 |

Decoded native values, own-key schemas, numeric cases (NaN classification and signedzero/infinity bits), fixed
role/count expectations and original input pointers are asserted before any
JSON/event/source projections. Expected patch roles and child-role sequences
are fixed by method/fixture/index, not inferred from output. Trade emission
checks all five native fill fields; holdings check accepted count, eleven root
fields and four info fields; snapshots/loaded patches check aliasing and flags.
No decode/reencode equality is presented as native proof.

## Findings corrected during development

The first source-oracle attempt asserted the original error for the method's
fresh missing-wallet error. Its process exit was not separately recorded;
source.err preserves the assertion. source2.json is an explicitly **stale,
pre-fix**80-fixture output: the snapshot fixture had overwritten the selected
fetchAndSetPositions source body with a child stub. That was a non-discriminating
oracle finding at the fixture assignment; it is fixed by binding that stub
only on private parent and asserting selected-method identity before invocation.
Both SHA-attached source gates and all final proofs use the corrected oracle.

Development native/native2/native3/native4 exits101 are compilation/setup
failures (float syntax/name collision, tick early-return type, tuple-field edit,
unused binding); they are not mutation proof. native5 exits101 with six passing
and two failing tests exposed the distinct mergeable expectation and snapshot
stub mistake. native6 exits0 with eight passing tests. These development runs
were on uncommitted work based on the original parent; their receipts are not
claimed as gates for a commit. Raw logs are retained compressed without editing.

Static sub-agent review read the eight source/Rust bodies, native/oracle tests,
control anchors and mappings; no execution. It found missing native child/cost/
patch checks, identical boolean witnesses, incorrect startup expectations and
output-selected patch schemas. Those were corrected. Review of the prepared
fixed-role tables caught external-public startup's unconditional tick call;
that expectation was corrected before the post-merge gate. Readers stopped
before mutation execution. No new production mismatch was reported at that
ordinary-fixture review floor.

## Scope limits

This slice ports eight method bodies against venue-local host capabilities.
Full exchange/API/private class construction and imported helpers remain their
separate rows. No claim covers exotic getters/proxies/iterators, arbitrary heap
alias graphs, exact browser scheduler behavior, signing implementation or live
venue requests. I did not run workspace builds/tests, host or wasm clippy,
wasm checks, monorepo suites, CI, old mutation matrices or live authenticated
requests. All commands executed serially; shared SDK types/traits were not edited.

## Delivery preservation

Push-time integration merge/preservation receipts are recorded separately in
this directory; they never relabel the gate SHAs above. Compressed artifacts
retain exact raw bytes, and artifacts.json records raw SHA256/length.

Push-time merge `772d8e05554a7d038b58a4128b6fc1bad442d363` incorporates
integration `9c663a908f40c2955e27b1ce58f97fb5c08fb331`. Source/tests diff
against the mutation SHA exits0. A34-path input audit retains strict differing
paths: foreign global manifests/exemptions/specification, Kucoin package script,
Bitrue lock dependency, and shared BaseWebSocket before-close hook. Other
inputs, including TypeScript/oracles, all numeric/crypto/type dependencies and
our manifests are byte unchanged. Because the shared exchange code changed,
all seven scoped gates were remeasured at this merge SHA: source80/check/
tests8/fmt/targets141+320/writer/extractor all exit0. Writer and extractor output
text reports zero updates/additions/orphans. See push-gates/gates.json and
preservation.json. The111 full-suite result stays attached to200d205d87; the
eight control results stay attached tof6c43d3bf7. Neither was rerun or relabeled
as covering the changed shared dependency.
