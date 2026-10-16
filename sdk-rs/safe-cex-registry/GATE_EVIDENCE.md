# P4a registry gate evidence

Measurement date: 2026-10-12. Counts below describe the tested tree at
`6fd427e73c`, not a live baseline; run parity:progress for the current corpus.

Branch: `feat/rust-p4a-registry`, based on `origin/feat/safe-cex-rust`.
The data slice maps 14 ported rows and one declared UTF-16 divergence; 134 rows
remain pending with the named P4a-core exchange-trait/venue-host blocker.
No constructor or provider lifecycle stub is claimed. The 26 venue manifests
are unchanged; each scaffold exports no item and claims zero ported rows.

## Registered-name census

The live TS AST equivalence oracle finds 47 names in identical web/native order.
At plan creation commit `73709ff193d8ad5b52b2658637777dabbf8aafcf` there were also
47 keys: 45 identifier keys plus quoted `lighter-robinhood` and `tealstreet-dex`.
Both are actual registered names, not Rust-only aliases. The plan's 45 matches
an identifier-only census; its author must correct the plan, which this branch
has not edited. Variant modules and constructor export names are compared
against both live TS registries, without executing their venue constructors.

## Negative controls, run individually and restored

| Gate | Mutation | Actual result |
| --- | --- | --- |
| fmt | Unformatted anonymous const in the Bybit scaffold | `cargo fmt --all --check` exit 1, named the diff |
| strict clippy | Dead private `unused_probe` in Bybit | Exit 101, named the unused function |
| scoped tests | Change the private data tag from `Data` to `Ping` | Exit 101, `specialized_updates_enforce_data_and_channel_literals` failed |
| TS equivalence | Change only Apex's constructor descriptor | Exit 1, `Rust registry differs from live TypeScript data` |
| layer policy | Remove Bybit's classification | Exit 1, `unclassified crate: workspace -> safe-cex-venue-bybit` |
| wasm policy | Remove Bybit's wasm classification | Policy build exit 1, named the unclassified workspace crate |
| wasm build | wasm-only `compile_error!` in Bybit | Exit 101, named `P4a wasm negative control` |
| coverage | Rename only pending `exchangeRegistry.bybit` to a nonexistent TS symbol | Extraction check exit 1, named addition and orphan; no write |
| invention/status | Export unmapped Bybit `invented_probe` | Check exit 1, named the invented public Rust item |
| report | Point only `GatewayMessage` at an unresolved target | Exit 1, refused Rust-index validation; no progress table |
| progress | Prefix the displayed report fraction with a spurious digit | Exit 101, `progress_agrees_with_validated_report_and_allocates_each_row_once` failed against report's unchanged total |

The progress mutation changes rendering, not report counting: the fixture's
report remains `1 / 1` while progress prints `11 / 1`. Its CLI regression also
asserts equality to the report total and rejects invalid mappings with no output.
Phase counters are produced by report's existing `Counts::add` path; progress
reads those counters and never iterates manifest symbols itself. P0/P7/P8/P9
zero-row buckets are explicitly not claims of completed phase milestones.

## Review corrections

Independent review identified a no-delimiter astral-key boundary: TS slices
UTF-16 and returns a lone high surrogate, while Rust truncates a scalar. The
helper is explicitly divergent, and both a Rust regression and live TS oracle
pin the difference. Generated colon-bearing Unicode keys still match.
Review also corrected phase allocation for `health/errorPatterns.ts` (P4),
secondary-public WS files (P3), and the existing HTTP-over-WebSocket root (P2).
The final independent review found no remaining blocker.

## Restored-tree validation (2026-10-12, `6fd427e73c`)

See the final section below for the actual positive outcomes; all deliberate
mutations above were restored before these checks and commits.

Final restored-tree results: workspace fmt and locked strict all-target clippy
pass. Scoped cargo tests pass for registry (4), parity (1 unit + 63 integration)
and all 26 empty venue crates. CI wiring/layer/wasm policy audits pass all 39
Node tests. The full wasm policy build includes all 26 scaffolds and registry
and exits 0. Source-driven TS registry/data equivalence passes 47 ordered names.

Using the installed Yarn 1.22.21 entrypoint (not the Volta shim), extraction
check exits 0 at 6901 rows with zero changes/additions/orphans; check exits 0
at 393 public Rust items; report and progress exit 0 and both show 277 / 6901.
Progress reads that fraction dynamically; no baseline denominator is pinned.
All owned TOML was parsed with Python tomllib, and the Rust manifest parser
and extractor validated the same tree. Existing lockfile dependency records
are unchanged; additions are only the 26 scaffold packages and registry.

No requested gate was skipped. The branch does not modify the parallel
`sdk-rs/safe-cex/src` directory, venue-family manifest rows, extractor source
roots, the plan of record, or mirror files. No PR was opened and nothing merged.

## PR 7213 review repair measurements (2026-10-12)

The earlier section is the historical `6fd427e73c` measurement. At repaired
implementation head `d90ee34d12`, the four owned files contain **12 ported,
3 divergent, 1 not-applicable, 133 pending** rows. Report and progress agree on
**275 / 6901**, with 331 distinct claimed Rust items, 49 divergent and 7
not-applicable rows corpus-wide. These are dated observations, not fixed targets.

The two promotions to divergent are deliberate: the channel preserves the
runtime `auth` cast outside the TS union, and GatewayMessage rejects explicit
null in typed optional fields that TS runtime casts tolerate. Native CommonJS
`require` is not-applicable. Gateway wire types now live in safe-cex-types;
thresholds/key helpers and the registry descriptors stay in safe-cex-registry.
Four constructors make fixed-tag envelopes usable by external producers.
At this historical head family/variant were stored labels outside the source-parity claim; the round-two repair below removes them entirely.

`yarn registry:check` now runs in rust.yml's **types** job after the full root
install. The wiring audit pins that step and the root script, rejects deletion
or failure masking, and push triggers include this branch. The oracle compares
TS-derived names, order, module paths and constructor exports, plus gateway
thresholds, key behavior and the declared auth/null boundaries.

Additional negative controls, each run alone and restored:

| Mutation | Actual failure |
| --- | --- |
| Overall numerator becomes ported + divergent | exit 101; both named progress integration tests fail |
| Overall numerator becomes claimed Rust items | exit 101; both named progress integration tests fail |
| Disable report-column schema guard | exit 101; reordered/extended-column regression fails |
| Delete Weex descriptor | oracle exit 1 |
| Repoint web bybitdemo to BybitExchange | wired oracle exit 1; web/native descriptor mismatch |
| Repoint web okxeudemo to OKXDemoExchange | wired oracle exit 1; web/native descriptor mismatch |
| Remove the workflow oracle step | CI wiring audit exit 1; missing named step |
| Remove Auth channel variant | exit 101; gateway auth regression fails |
| Reverse ticker constructor's plural-channel choice | exit 101; external-producer regression fails |
| Allow explicit null symbol | exit 101; declared-null-divergence regression fails |

Progress fixtures now contain 1 ported row, 3 claimed items (a real split),
1 divergent row, 1 not-applicable row and 2 pending rows. Both library and CLI
checks assert the exact fraction and pending remainder against validated report.
The renderer rejects an altered header before interpreting positional columns.

Restored checks: scoped types/registry/parity fmt, plain strict all-target clippy,
all-target tests (types 9 wire + 7 generator; registry 6; parity 2 unit + 63
integration), types+registry wasm build, 42 Node wiring/layer/wasm-policy tests,
live registry oracle (47 names), extractor (zero changes/additions/orphans),
check (397 public Rust items), report and progress all return exit 0.
The mutation-affected test suites and live oracle were rerun after restoration.
No full-workspace Cargo build or monorepo suite was run during this repair.

Own adversarial review at `d90ee34d12` used a fresh detached checkout and
returned **MERGE**, no required fix. It passed 35 scoped Node tests, 6 registry
tests and 5 progress tests. Its independent claimed-count substitution broke
both progress regressions; removing the registry CI step failed the wiring
audit. Both mutations were restored and positive tests reran green.


## Round-two repairs, measured 2026-10-12

The GatewayMessage record now declares whole-frame validation versus TS's pure
cast, including missing/null required fields, unknown enums, wrong types, and
unused malformed fields rejecting otherwise usable frames. Cargo pins those
examples. Ticker constructor documentation now says the caller independently
selects ticker/tickers; the other three constructors retain fixed channels.

Removed the 94 stored family/variant labels. A 47-entry source-extracted snapshot
now checks every ordered Rust descriptor and registered-name output in plain
cargo test independently of the live oracle. Repointing Weex's module makes
that named test fail (exit 101); restored tests pass (8 registry tests).

The mandatory registry:check command now also runs oracle negative controls.
They feed valid complete example output with exactly one changed demo module or
constructor through the same checker, require exit 1 and the descriptor-mismatch
diagnostic, and reject a success message. Deleting the descriptor assertion
makes this test fail (exit 1). The workflow audit now also rejects moving the
oracle before root install. Node audit plus oracle tests: 37 passed. Plain scoped
Clippy passed. Real-tree parity check: 6901 rows / 397 public items; extraction:
zero changed, added or orphaned rows. These are dated measurements, not constants.


### Final round-two controls (2026-10-12)

The live oracle now compares the TS GatewayChannel literal union to serialized
Rust variants minus Auth. The final control below replaces the separately maintained inventory with
Serde-derived enum metadata. Adding auth to the TS union, removing trade, and adding funding
each fail exit 1 with their channel-specific diagnostic. The auth cast remains
required. The source GatewayMessage cast is also extracted and evaluated on valid
JSON containing symbol:null: TS preserves it while Rust rejects the same frame.
A future replacement of that direct cast requires revisiting the declaration.

Progress fixture counts are now 1 ported / 2 divergent / 3 not-applicable /
4 pending (10 total, 4 distinct Rust items). Both ported+divergent-na and
2*ported-na mutations fail both named progress tests (exit 101), then restore.
This discriminates the reported mutations; no single fixture proves the absence
of every possible arithmetic expression that happens to match its counts.

The wiring audit rejects a registry step env override as well as moving it ahead
of root install. Limitation: the audit is bootstrapped from the native routing
step; deleting that invocation or job can prevent the audit from running at all.
Its removal/masking guarantees apply when the audit itself executes. That
pre-existing bootstrap weakness is not repaired by this PR.

The 277-to-275 drop is exactly TWO ported-to-divergent moves (GatewayChannel and
GatewayMessage). require moved pending-to-not-applicable, changing pending
6571-to-6570 and NA 6-to-7, with no effect on the ported numerator. The require
note explicitly exempts only the ambient host declaration, not its loader or
any constructor implementation.


### Serialized inventory residual (2026-10-12)

Measured the requested hole before repair: adding Funding to GatewayChannel and
to the exhaustive match, but omitting it from the array, left the live oracle at
exit 0. Replaced both array and match with a metadata-only Serde Deserializer in
the example. GatewayChannel's derived Deserialize supplies its canonical wire
variant list to deserialize_enum; the example captures that list directly.
There is no separately authored list to forget. No production API or dependency
was added. Adding Funding to the Rust enum now makes the oracle exit 1 with
GatewayChannel members differ beyond declared Auth extension. Restored oracle
negative controls and scoped Clippy pass.


## Round-three oracle repairs (2026-10-12)

Both registry scripts invoke cargo directly and no longer read the CARGO
executable override. Running the full oracle-control test with CARGO set to a
nonexistent executable passes. A workflow/job/GITHUB_ENV CARGO assignment cannot
select forged output through that removed hook. The audit also pins exact
commands in the types job's toolchain isolation and root dependency install.

The bad-input control now covers each behavior in the success message: ordered
names, descriptor module/export, both thresholds, subscription keys, channel
members/auth extension, auth reply, typed-null rejection and astral truncation.
A temporary native registry fixture diverging from web must fail too. Deleting
threshold, key, native-equality or channel-member assertions individually makes
the oracle-control test fail (exit 1); restored controls pass. Duplicate TS union
members are deduplicated and a duplicate-member fixture stays green.

GatewayMessage's note now explicitly records unknown top-level fields being
accepted and discarded on typed round-trip; a seq field regression pins the
loss. The TS cast retains the original parsed object's extra fields.

Correction to the prior numerator evidence: the enlarged fixture improves
separation, but it must not be credited with first detecting the report-layer
mutants. The verifier reproduced ported+divergent-na against the earlier head
and the full suite already rejected it, including two unrelated report tests.
Our recorded local mutations changed progress::from_report and failed the two
named progress tests; that narrower experiment was not proof that report()'s
corresponding mutation had survived the full pre-repair suite. Keep the enlarged
fixture and arithmetic-coverage hedge, without claiming it uniquely closed an
existing full-suite hole.
