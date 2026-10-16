# Trait/API declaration yield

Through rank **30**, canonical diagnostics are **4673**, down **1222** from the
frozen 5895 baseline with zero additions. Status movement remains zero. The
running authoring denominator is **173 family-row edit events across 168 distinct
rows**. The table below separates API yield from rename/provenance collateral.

## Running yield

| Ranked batch | API cleared | Other cleared | Total cleared | Row-edit events | Distinct rows | API / edit event | Total / edit event | API / family |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1–10 | 495 | 79 | 574 | 69 | 69 | 7.174 | 8.319 | 49.5 |
| 11–20 | 287 | 76 | 363 | 58 | 53 | 4.948 | 6.259 | 28.7 |
| 21–30 | 214 | 71 | 285 | 46 | 46 | 4.652 | 6.196 | 21.4 |
| Cumulative | 996 | 226 | 1222 | 173 | 168 | 5.757 | 7.064 | 33.2 |

An edit event means one family annotating one existing source row. Five rows
receive annotations from two families in batch two; counting them only once
would inflate that batch's labor yield to 6.849 diagnostics per distinct row.
The first and third batches have no repeated rows. None of the three batches
shares a changed row with another. Cumulative yield per distinct row is 7.274;
it is not the event-based decay metric. Two source-role wording corrections
after rank 30 are separate from these family-row events. Including those two
editorial events gives a third-batch yield of 285/48 = 5.938 and cumulative
1222/175 = 6.983. Both denominators remain visible so review rework is counted.

The overseer's arithmetic is correct as a flat-rate scenario: 5895 divided by
574/69 is **708.6**, approximately 700 row edits. At the newly measured cumulative
937/127 rate the same scenario becomes **799.0** events. Neither is a measured
calendar forecast: selection favors large families, the tail average is lower,
other causes have no equivalent authoring measurement, and three reviewed members
still lack established production callers. The elapsed-time observation below
supplies a separate constant-rate scenario, not a controlled calendar forecast.
These batches change annotation and layer
classification only; it does not independently verify the separate audit's
behavior-defect percentage or generalize these rows to every remaining defect.

The original family mean is 2563/250 = **10.252** diagnostics per family. After
batch one the remaining mean is 2068/240 = **8.617**. After batch two the trait
group retains **1781 diagnostics across 232 families**: 230 unexamined families
and two examined families retaining one unassigned member each. Eighteen families
are fully covered; twenty have been examined. The next untouched rank is **21**,
with 23 diagnostics. After batch three the trait group retains **1567 diagnostics
across 223 families**: 220 untouched families and three partially assigned ones.
Twenty-seven families are fully covered; thirty have been examined. The new
stopping floor is rank **30**; rank 31 is Orderly data_mappers::DataHost with 20
diagnostics. This remains a finite ownership audit.

## Elapsed lane-time measurement through rank 20

The inclusive local wall-clock window is **2026-10-15 18:04:32–18:51:58
UTC+08:00**, **47m26s = 0.79056 lane-hours**. Its endpoints are the branch
creation reflog at reviewed ancestor `5e658c56f75affb85ced13d45b668ee69e6baccd`
and evidence commit `08c72ff912b3fff30460e0f035e7bc70bdb02306`. Git commit times
and the local branch reflogs provide the clock evidence; this is elapsed primary
lane occupancy, not measured typing time or summed concurrent sub-agent hours.
It includes preparation, source reading, authoring, review, gates, documentation,
merge repair, and the entire gap between batches. Earlier systemic research is
outside this window. Local clock accuracy and work before branch creation were
not independently measured.

| Window, UTC+08:00 | Seconds | Diagnostics cleared | Diagnostics / elapsed hour |
| --- | ---: | ---: | ---: |
| First branch creation 18:04:32 to first receipt `3d527d2d21` at 18:18:20 | 828 | 574 | 2495.7 |
| Continuation branch creation 18:27:35 to receipt `0b75aff70a` at 18:41:50 | 855 | 363 | 1528.4 |
| Merge repair and fresh receipts, 18:41:50 to `08c72ff912` at 18:51:58 | 608 | 0 additional | 0 |
| Between-batch interval retained in inclusive denominator | 555 | 0 additional | 0 |
| Inclusive window | 2846 | 937 | **1185.2** |

The first and second authoring windows show a marginal throughput decline;
charging the shared merge-repair time to the second batch gives **893.2/hour**.
Excluding only the between-batch interval gives 1472.4/hour, but the conservative
headline retains that interval because its active/idle split was not measured.
The earlier 709-event and later 799-event estimates reuse overlapping data;
they are not independent confirmation of a forecast.

At the inclusive observed rate, **4958 / 1185.2 = 4.18 lane-hours** is the
constant-rate scenario. At the second-batch-plus-merge rate it is **5.55
lane-hours**. Neither number establishes completion within one working day:
selection favors large families, the tail is less efficient, 2113 baseline
diagnostics outside the three structural groups have no measured authoring rate,
and unresolved ownership can require engineering rather than declaration work.
No measured distribution yet supports a week/month bound. This measurement
updates the earlier absence of a lane-time observation without retroactively
claiming a controlled productivity experiment or a fresh runtime proof.

At the subsequent GitHub observation, PR 7957 is OPEN and **MERGEABLE** at
`7aae8b5f3d356bb3a67b71c84e3e8e2973417cf3`; that says conflict-free, not CI-green.

## Ranks 1–10: frozen receipt

The first ten largest families from the frozen corpus received explicit
API ownership on existing consuming operations. Their canonical diagnostics fell to
**5321: 574 removed, zero added**. Of those removals, **495** are the selected
trait/associated-item ownership diagnostics; **61** are existing operation-name
declarations and **18** are existing source-provenance declarations completed by
the same notes and file headers. Validation severity, exemptions and Rust/TS
production behavior are unchanged.

The measured yield is **57.4 diagnostics per family**, including collateral;
the narrowly attributable API yield is **49.5 per family**. This required
**495 individual target annotations across 69 existing symbol rows**, not ten
single-row edits. Corresponding total yields are **1.160 per added target** and
**8.319 per changed symbol row**. These denominators bound the authoring claim;
they do not establish a constant repair cost for the remaining corpus.

Baseline source revision: `305b9e7158af1d2ffaed0648579a2c9801c155c7`.
Its merge parent is integration `d4e20845a85cc4e2711c99648ef7547f8aa806df`.
The branch also includes the previously sub-agent-reviewed observational/index
regression work in PR 7944, still open when measured. This slice depends on that
PR's inventory tool; it introduces no unreviewed ancestor work.

Each row below is measured **after committing that family** with the direct
maintained `parity-check --workspace sdk-rs` executable. Every exit is **1**;
no partially repaired corpus is described as passing. Counter subtraction checks
both removed and added diagnostics, including duplicate multiplicity.

| Family | API targets | Changed rows | Total removed | Extra removed | Remaining | Measured revision |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| GRVT private_feeds::PrivateFeedHost | 63 | 9 | 71 | 8 | 5824 | `36d42e6f94700bed6b47e893ef617f51060f9026` |
| GRVT builder_workflow::BuilderHost | 52 | 7 | 52 | 0 | 5772 | `28caa1ef4c07a22ca8eee251e7b464674dee6131` |
| WOO legacy_fill_recovery::Host | 51 | 8 | 61 | 10 | 5711 | `6603129134ad41e37be774400c9be2f1b732a741` |
| WOO legacy_socket_control::Host | 51 | 6 | 65 | 14 | 5646 | `ac634ddcbc98c73aa184f85b9699a61a1ff78fad` |
| Bitrue collection_reads::ReadHost | 49 | 7 | 56 | 7 | 5590 | `a2af0eb5841a52b08b71fe8d6170a901af7d89e2` |
| WOO legacy_order_writes::Host | 49 | 5 | 54 | 5 | 5536 | `483e3e3515746fe34ae639dce9986018600f2ec7` |
| Backpack rest_reads::Host | 47 | 8 | 63 | 16 | 5473 | `8ee06ad5679a7e680ecec578cd6823b7f0fba7c0` |
| WOO legacy_data_reads::Host | 47 | 6 | 53 | 6 | 5420 | `594b954586a2c3f3c7fc44d414c9ba950986af9c` |
| Kraken rest_account::ReadHost | 46 | 7 | 53 | 7 | 5367 | `8320876678fb3114ee95b0e56b23191ddaf989ac` |
| WOO legacy_order_control::Host | 40 | 6 | 46 | 6 | 5321 | `544a8a8ae60b8a5228162be80a7168eb590b4c8c` |

Selection stops at iteration **10**, ordered by descending diagnostic count and
then target path. The next family has 38 diagnostics. The original trait/API
group has **2068 diagnostics across 240 remaining families** after these repairs;
this is a structural inventory, not a proof that those families admit identical
ownership decisions. The remaining non-trait causes are not characterized anew by this slice.

Each added target is assigned to a source operation that consumes it directly or
through local helpers, with operation-specific roles in its existing note.
Examples requiring actual call-site decisions: GRVT FillKey follows fill
deduplication, template follows js_template!, WOO map_algo follows the algo=true
helper branch, and captured transport/logger/iterator types follow consumers of
those capabilities. Shared API declarations do not claim a concrete runtime host
implementation or complete a pending class. Existing status/layer/body targets
and prior note prefixes are preserved. No representation-policy ruling changes;
its authority remains [MANIFEST-SPEC](MANIFEST-SPEC.md#divergence-taxonomy).

Independent tomllib comparison of all 512 manifests preserves all 7341 source
identities and the exact status census **4802 ported / 2228 divergent / 304 pending /
7 not-applicable**. **Zero status rows moved.** Only 69 rows' rs/note fields change.
Four provenance entries are added: Backpack REST reads, WOO private fill recovery,
WOO private socket control, and WOO public socket control.

Focused verification source revision: `b2b33b4f9718c63960f8704401ee4e77212de22b`.
The later evidence commit changes documentation only. Commands run from the
repository root; Cargo invocations are sequential with CARGO_INCREMENTAL=0.

| Gate or control | Exit | Observed result |
| --- | ---: | --- |
| Scoped `cargo build --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-parity --bin parity-check --bin parity-trait-inventory --jobs 2`, baseline revision | 0 | Finished dev profile; builds only the two named tools |
| Scoped `cargo check --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-parity --all-targets --jobs 2` | 0 | Finished dev profile |
| `node --import tsx sdk-rs/parity/scripts/trait-declaration-controls.mjs`, baseline | 0 | 11 tests executed and passed |
| Same control, ten owner mappings removed in one batch | 1 inner test exit | All ten `TRAIT native complete ownership: <family>` assertions fail; checker separately reports each exact owner's invention diagnostic; all 11 tests execute and fail |
| Same control, exact bytes restored | 0 | All 11 tests pass; control driver exits 0 and checks exact restoration |
| `yarn parity:extract` through the real Yarn entrypoint | 0 | 7341 symbols, 512 files, updated 0 manifests; 0 added and 0 orphaned |
| `yarn parity:extract:check` through the real Yarn entrypoint | 0 | Same source census, coverage gate passes |
| `node --check` for the new test and control scripts | 0 each | Syntax accepted |
| Scoped `yarn eslint` for the three new scripts | 1 | Tool unavailable: Command "eslint" not found; no lint pass claimed |

The native assertions decode actual manifests before calling the maintained
checker. Independent literal family floors precede exact AST-inventory/manifest
set comparison, and ten literal consuming-operation/member pairs pin meaningful
placement. The ten negative controls remove **owners**, not 495 independent
members; no complete semantic mutation matrix or fresh runtime parity proof is
claimed. At that first-batch revision, the global gate remained red with 3259 unmapped-public, 1186 rename-target,
644 undeclared-file, 126 unresolved-target, 68 missing-layer, 36 unused-file and
two stale-exemption diagnostics.

Read-only sub-agent review at b2b33b4f97 returns **MERGE**. It independently
compares the frozen parent, all decoded row changes, unique API targets, raw
multiset subtraction and completed control logs, with finite source/role review.
It runs no Cargo, tests, mutations or edits. No concrete mapping/control blocker
remains from that review. Raw logs and observational authoring intermediates stay
ignored under target; this manually authored sheet records the measured claims.

Not run for this annotation slice: venue runtime/oracle suites, workspace build,
host or wasm Clippy, wasm builds/tests, full suites, or new canonical report and
progress commands. Their exits are not inferred from the direct check.

## PR 7957 integration merge-down

Integration `694e07f7e049a5a7047c41e994a0fc854dc78a59` merges into this branch
as `c6115990b95ebc66d0d9b8821249cd337cc80822`. Git's merge strategy combines the
Kraken manifests automatically; no manual conflict edits or dropped paragraphs
are needed. PR 7944 is now merged. The exits below belong to **c6115990b9**, not
to the earlier pre-integration receipts.

Archived integration's 512 manifests validate against the merged production
source index with exit **1**, reproducing the exact original **5895** diagnostic
multiset (zero additions/removals against the frozen baseline). The merged PR's
direct checker exits **1** with **5321**: **574 removed, zero added**, preserving
the original measured yield. Independent tomllib comparison against that actual
integration parent confirms 69 changed rows, preserved original targets/notes,
zero status or layer changes and unchanged 4802/2228/304/7 census.

| Rerun at c6115990b9 | Exit | Result |
| --- | ---: | --- |
| Scoped two-tool Cargo build, same locked manifest/package/jobs arguments above | 0 | Finished dev profile |
| Scoped parity all-targets Cargo check | 0 | Finished dev profile |
| Scoped parity all-targets host Clippy with `-- -D warnings` | 0 | Finished dev profile; not workspace or wasm Clippy |
| Ten-owner control driver, baseline / mutated / restored | 0 / 1 / 0 | 11 tests run each phase; ten named native failures and ten invention diagnostics; exact restoration; driver exits 0 |
| Real Yarn extractor writer / check | 0 / 0 | 7341/512 census; zero updates/additions/orphans in both inspected outputs |
| Direct canonical check | 1 | 5321 diagnostics; unchanged annotation gain |

These reruns do not claim workspace builds, venue runtime/oracle suites, wasm
checks, full suites, ESLint, or new report/progress exits. Incoming venue audits
are integration changes, not gains authored or behavior proven by this PR.

## Ranks 11–20: continuation

This slice builds on `3d527d2d2139b0041bbd51986f1e61636fed7e7a` (reviewed,
queued PR 7957, itself dependent on reviewed PR 7944). Integration remained
`d4e20845a85cc4e2711c99648ef7547f8aa806df` at branch creation/merge-down.
No unreviewed ancestor work is asserted as a new annotation gain.

Fresh direct baseline at that parent exits **1** with **5321** diagnostics.
Each committed family below is measured with the same direct checker and exits
**1**; every step has zero added diagnostics. This batch clears **287 API +
46 rename + 30 provenance = 363** diagnostics. Cumulative removals are
**782 API + 107 rename + 48 provenance = 937**.

| Rank and family | API cleared | Unassigned | Row-edit events | Total cleared | Remaining | Measured revision |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| 11 WOO legacy_market_reads::Host | 37 | 1 | 7 | 44 | 5277 | `c3c551e5039cc7df8fa18e01f86ae30319aa7716` |
| 12 GRVT public_feeds::PublicFeedHost | 36 | 0 | 5 | 36 | 5241 | `f4729f60c3eab2cc9b25664a3f3bfcd7964abd59` |
| 13 GRVT account_runtime::AccountHost | 30 | 1 | 8 | 48 | 5193 | `ced541990a59e84f0c04c63be136b255e9316930` |
| 14 GRVT private_lifecycle::PrivateLifecycleHost | 28 | 0 | 5 | 38 | 5155 | `0d5801d2f47beb0fcd902549af2c6a243ef3c4ed` |
| 15 WOO legacy_metadata::Host | 28 | 0 | 7 | 35 | 5120 | `4fffae7f19c3f0c5af915900319f03874f0fdfb5` |
| 16 GRVT session_host::SessionHost | 27 | 0 | 8 | 28 | 5092 | `4d9f013d1c76a589daef5125f99f9e2a0dcc0ebe` |
| 17 Backpack startup_recovery::RuntimeHost | 26 | 0 | 3 | 33 | 5059 | `ab9827855567233d7d44cf051e63a02c66b5ca34` |
| 18 Bybit core_snapshot_readers::SnapshotHost | 26 | 0 | 6 | 32 | 5027 | `26b0ffd3bdfd661075eef44e3946df7e2fb43ff0` |
| 19 Hyperliquid spot_account_reads::SpotAccountHost | 26 | 0 | 7 | 42 | 4985 | `21e2764dd9c89ee431fc9b3d583e06d1655b3d39` |
| 20 Backpack public_listeners::Host | 23 | 0 | 2 | 27 | 4958 | `55ac071cc45eda9b0d1d936393a224910140dbff` |

All 7341 source identities across 512 manifests retain their statuses:
**4802 ported / 2228 divergent / 304 pending / 7 not-applicable**. Exactly 53
existing rows gain targets/notes, preserving prior target order and note prefixes.
Six touched Bybit rows additionally change mirror to adapted because the newly
declared value/method/Promise host split is function-level adaptation. This is a
layer correction, not a status gain. Eight source-file provenance entries are
added across six manifests. No production Rust/TypeScript or exemption changes.

Real ownership decisions include GRVT SessionHost's shared-supertrait members
across API/account/private-lifecycle source files; trading-only equals_number,
is_function, null and promise belong to createTradingAPI, not createAPI's market
branch. Backpack RuntimeHost follows start rather than the unrelated constructor;
Market/Subscriber follow subscribeToAllMarkets. Backpack Throttle follows book
cleanup. Hyperliquid candle and fake-position members follow their specific
runtime-bound AccountCallback branches. These are explicit declarations of
consumed capabilities, not claims of completed host runtime implementations.
Policy definitions remain at [MANIFEST-SPEC](MANIFEST-SPEC.md#divergence-taxonomy).

One finding remains deliberately unassigned and still fails canonical invention; the second is superseded:

- `safe-cex-venue-woo/src/legacy_market_reads.rs::Host::nullish`: declared as a
  required public capability, but no production algorithm call site was found.
- `safe-cex-venue-grvt/src/account_runtime.rs::AccountHost::number_value`:
  superseded. `trading_runtime.rs` calls it through `TradingHost: AccountHost`
  (placeOrder status, fill size), so it is now mapped on `GrvtExchange.placeOrder`.

Neither is proved dead for external implementors of these public traits.
Deleting the public API or assigning a fictitious consuming operation merely to
clear the diagnostic is unsupported. The focused checks require both exact
members to remain unmapped and both exact invention diagnostics to remain visible.
They are not suppressed classes or newly fabricated manifest rows.

Verification inputs are committed at
`87fda8ed6014d742a09cb819b478beee2ef1f5ac`; later evidence edits are documentation
only. The direct step-20 checker receipt belongs to 55ac071cc45 as listed above.
No Cargo invocations overlap within this lane; incremental compilation is disabled.

| Continuation gate/control | Exit | Measured result |
| --- | ---: | --- |
| Scoped `cargo check --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-parity --all-targets --jobs 2` | 0 | Finished dev profile |
| `node --import tsx sdk-rs/parity/scripts/trait-declaration-controls.mjs 11`, baseline | 0 | 21 focused tests pass; literal ranked-family floor stops at 20 |
| Same driver, ten owner mappings omitted together | 1 inner test exit | Ten named `TRAIT native declared ownership: <family>` failures, each exact invention diagnostic, and selected canonical-remainder failure; 21 tests execute, 11 fail |
| Same driver, owner mappings restored | 0 | All 21 focused tests pass; exact input bytes restored |
| Same driver, two unassigned members falsely claimed together | 1 inner test exit | Two named `TRAIT unassigned capabilities remain unmapped: <family>` failures plus canonical-remainder failure; 21 tests execute, three fail |
| Same driver, false claims removed by exact restoration | 0 | All 21 focused tests pass; driver exits 0 and verifies exact bytes |
| `yarn parity:extract` through the real Yarn entrypoint | 0 | 7341 symbols/512 files; updated 0, added 0, orphaned 0 |
| `yarn parity:extract:check` through the real Yarn entrypoint | 0 | Same census; would update 0, added 0, orphaned 0 |

The controls prove ten owner omissions and two false ownership claims, not 287
independent runtime mutations. Native decoded assertions run before the maintained
checker. The explicitly unassigned members remain required in the canonical
remainder, so a blanket claim that makes the global count smaller fails these
focused tests. The checker itself is unchanged and remains exit 1 with **4958**:
2972 unmapped-public, 1140 rename-target, 614 undeclared-file, 126 unresolved-target,
68 missing-layer, 36 unused-file and two stale-exemption diagnostics.

Read-only adversarial sub-agent review at 87fda8ed60 returns **MERGE**. It
independently checks all source identities, the 53 row changes/287 target additions,
six explained layer reclassifications, per-step and total Counter subtraction,
both completed control batches and exact restoration. Its source/role review
corrected Throttle ownership before commit. It runs no Cargo/tests/mutations or
edits and makes no fresh full semantic parity claim.

At 87fda8ed60, after observing no compiler working directory in this worktree,
the incremental cache was confirmed absent (zero bytes removed). Generated raw receipts and
observational authoring intermediates remain ignored. Not run for this continuation:
new binary builds, venue runtime/oracle suites, workspace builds, host/wasm Clippy,
wasm lane, full suites, ESLint or new canonical report/progress runs. The prior
ESLint-unavailable attempt is recorded only under its actual first-batch revision.

## Continuation after repaired parent merge

The continuation merges repaired PR 7957 head
`7aae8b5f3d356bb3a67b71c84e3e8e2973417cf3`, including integration 694e07f7e0,
as **`ef60b8fa0792849775c7488ee5bde7fdd19bf9fb`**. The sole conflict is between
this sheet's historical continuation and PR 7957's newer gate append. Both are
retained. The earlier empty-cache observation is explicitly bound to 87fda8ed60.

Independent frozen-parent comparison against **7aae**, rather than d4 or 3d,
confirms the same 53 changed rows, 287 added targets, six explained Bybit layer
changes and zero status movement. Incoming target order and note prefixes remain
intact; incoming audit files are unchanged. The new direct check exits **1/4958**,
removing **363** from the repaired parent's 5321 with **zero additions**. It is
also the exact pre-merge 4958 multiset, with zero additions/removals, so none of
the running yield is attributed to integration's separate audit work.

Fresh gates measured at **ef60b8fa07**, all completed:

- Scoped locked parity all-targets Cargo check, jobs 2: **0**.
- Twenty-family native/canonical baseline: **21 tests pass, exit 0**.
- Ranks 11–20 owner-omission batch: **exit 1**, ten named native ownership
  failures plus selected canonical-remainder failure; restored **21 pass/0**.
- False-ownership batch for the two unassigned members: **exit 1**, two named
  native unmapped-capability failures plus canonical-remainder failure; restored
  **21 pass/0**. Both controls verify exact bytes; driver **0**.
- Real Yarn extractor writer/check: **0/0**, inspected 7341/512 census and zero
  updates/additions/orphans in both outputs.
- Canonical checker: **1**, 4958 diagnostics, not a global gate pass.

No new binary build, Clippy, workspace build, venue oracle/runtime suite, wasm,
full suite, ESLint or report/progress exit is claimed at ef60. The scoped host
Clippy at c611 belongs to the repaired parent, not this source revision. The
running yield table at the top remains unchanged by the merge.

## Ranks 21–30: continued ownership authoring

Parent `08c72ff912b3fff30460e0f035e7bc70bdb02306` is the reviewed continuation
from PR 7967, depending on reviewed PR 7957. Both remain queued when this batch
starts; there is no unreviewed ancestor work. Integration is
`694e07f7e049a5a7047c41e994a0fc854dc78a59`, already included in that parent.
This branch changes no production Rust/TypeScript code, checker severity,
exemptions or source status. Policy authority remains
[MANIFEST-SPEC](MANIFEST-SPEC.md#divergence-taxonomy).

The ten next families remove **285 diagnostics: 214 API ownership, 45 existing
operation rename declarations and 26 provenance declarations**, with zero
additions. The direct checker remains exit **1**, falling **4958 -> 4673**.
There are **46 family-row edit events on 46 distinct existing rows**. None owns
an API from the preceding twenty families, so cumulative distinct rows are 168.
Nine families are fully assigned; MEXC's `ExchangeRuntimeHost::array` has no
production algorithm call in its module and stays unmapped and diagnostic.
No test-host implementation is substituted for a production consumer.

| Rank and family | API assigned | Row events | Total removed | Remaining | Measured revision |
| --- | ---: | ---: | ---: | ---: | --- |
| 21 MEXC exchange_runtime::ExchangeRuntimeHost | 22 | 6 | 36 | 4922 | `57130700ff55aab56114c6ed46628761fbd6578e` |
| 22 Orderly ws_controls::WsHost | 23 | 5 | 28 | 4894 | `e6c978bd4e10107d41f0a10256f17d41866ed201` |
| 23 Bitrue trading_rest::TradingHost | 22 | 7 | 29 | 4865 | `a8b30735cd61e27ab8b39e5c6d44a601b4112451` |
| 24 Orderly runtime_actions::RuntimeHost | 22 | 5 | 27 | 4838 | `6d22035b35b4f2df5fdc6442775fc30db0cf6c06` |
| 25 Backpack api_factory::Host | 21 | 1 | 21 | 4817 | `d689b8c411a9e42c3ce571e810f0fc338b93a72b` |
| 26 GRVT composition::CompositionHost | 21 | 3 | 30 | 4787 | `67cc5c82281172e03f0d8132f3ba68bd58d6062e` |
| 27 Hyperliquid balance_state::BalanceHost | 21 | 7 | 38 | 4749 | `47edbfcdfa5b807403ba661188c2a1692339b517` |
| 28 Kraken message_dispatch::MessageHost | 21 | 3 | 24 | 4725 | `5fc53e51158e384ac6301c39039597f0043ecfb8` |
| 29 Lighter exchange_state::ExchangeStateHost | 21 | 7 | 28 | 4697 | `d8b0feab60ee612e43537eeeac7face545dd36df` |
| 30 Backpack order_actions::Host | 20 | 2 | 24 | 4673 | `02e8f9f74b4e7ef23029718b78b4c1f5ca678396` |

Every table entry measures the committed revision with the maintained checker;
every exit is 1 and every Counter subtraction has zero added diagnostics.
The cumulative running yield is now **1222/173 = 7.064 diagnostics per family-row
event**, compared with batch yields **8.319 -> 6.259 -> 6.196**. Counting the two
later editorial corrections gives **1222/175 = 6.983**; this distinction is not
an estimate of every keystroke or uncommitted edit. The flat all-baseline scenario
at the family-row rate becomes **5895/(1222/173) = 834.6 events**, compared with
708.6 and 799.0 at the earlier checkpoints. The same-data and tail caveats above
still apply. API yield per family decreases **49.5 -> 28.7 -> 21.4**.

The native fixture now stops at literal rank 30. Exact AST family floors and ten
literal consuming-operation witnesses precede canonical assertions. Two paths
need more than a free-function textual search: Orderly `WsHost::Error` is the
actual `Result<H, ()>` error projection in private connection, and
`RuntimeHost::add` increments the retained `TickCallback::invoke` argument
scheduled by `tick`. Source and annotation review covers all ten families.

Read-only sub-agent review found two inaccurate role descriptions, with valid
underlying targets: Bitrue's `null_is` tests the parsed maximum leverage before
updating the market limit, and MEXC's `equal` compares the live order symbol in
its retained filter. Commit `17bc51c52e689df79c28a3a057bbbc2dafc8d0ea` corrects
only those phrases. No status, target, layer or validation rule is changed by
that follow-up. These two review fixes remove zero further diagnostics.

At source/test revision **1953a15a72f09bbff5464576c6792910c95670a2**:

| Gate | Measured exit | Result |
| --- | ---: | --- |
| Scoped locked parity all-targets Cargo check, jobs 2, incremental disabled | 0 | Finished dev profile, 0.27s; not workspace compilation |
| Thirty-family native/checker baseline | 0 | 31 tests pass |
| Remove the ten new owner mappings together | 1 inner; 0 control driver | Ten named `TRAIT native declared ownership: <family>` failures plus the maintained selected-ownership assertion; 31 tests execute, 11 fail; every owner also has its exact invention diagnostic |
| Exact restoration after owner omission | 0 | All 31 tests pass; exact original bytes restored |
| Falsely claim MEXC's unassigned array | 1 inner; 0 control driver | Named `TRAIT unassigned capabilities remain unmapped: ...ExchangeRuntimeHost` and maintained selected-ownership assertions fail; 31 execute, 2 fail |
| Exact restoration after false claim | 0 | All 31 tests pass; exact original bytes restored |

These controls discriminate owner omission and one false claim; they are not
214 independent member mutations or new runtime behavior proofs. Raw logs stay
ignored under target, including the SHA/index-21 control directory.

At corrected-note revision **17bc51c52e689df79c28a3a057bbbc2dafc8d0ea**:

| Gate | Measured exit | Result |
| --- | ---: | --- |
| Focused thirty-family native/checker target | 0 | All 31 tests pass after the two note corrections |
| Extractor writer | 0 | 7341 symbols / 512 sources; updated 0, added 0, orphaned 0 |
| Extractor check | 0 | Would update 0; coverage passes, same 7341/512 census |
| Direct canonical check and strict diagnostic census | 1 / 0 | 4673 diagnostics; exact same multiset as rank 30, zero delta |
| Independent archived-snapshot tomllib audit | 0 | All 512 manifests / 7341 rows; only 46 rs/note rows and seven provenance entries change; all original target prefixes and note prefixes retained; zero status/layer changes |

The census remains **4802 ported / 2228 divergent / 304 pending / 7
not-applicable**, with **zero status rows moved**. Remaining diagnostic classes
are 2758 unmapped-public, 1095 rename-target, 588 undeclared-file, 126
unresolved-target, 68 missing-layer, 36 unused-file and two stale-exemption.
The initial observational row-audit attempt failed on a manifest with no symbol
array; the successful audit handles empty symbol arrays and compares archived
commits rather than a moving mutation-control tree. Its failed attempt is not a
passing gate.

After the Cargo check, no compiler with a cwd inside this worktree was observed.
The incremental directory was empty (zero bytes removed); absence was verified
at the corrected-note revision. That is a point-in-time cleanup observation.
No new workspace build, workspace/host/wasm Clippy, venue runtime or oracle tests,
wasm lane, full suite, ESLint, or canonical report/progress invocation ran for
this batch. The earlier scoped Clippy receipt remains bound to c6115990b9.
