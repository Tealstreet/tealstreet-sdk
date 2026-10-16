# Backpack cardinality audit

Scope: local follow-on branch `feat/rust-venue-backpack-auth`, based on reviewed
request-signing slice edf2aab4570f5bc98a36b88dc65b57567481c9ae. Integration
ba6468e0076f360c9331607dd433896f442e5e6b was merged at
5b23a375dac4473aad579deecded4b0571ebc96f after all running mutations restored.
PR7269 stays pinned at8ecb30b0085043c5ea2b2705f8c08b9ec2fc6d19; these additions
are not asserted to exist in that PR.

## Finding and census

Four existing witnesses lacked independent source-corpus counts: types,
API helpers, signing preimages, and auth. Removing every case or one case from
one source subcorpus in each target still passed. These were eight controlled
Rust-harness mutations after actual TypeScript execution, not production
mutations and not edits to source or fixture values.

Inspection found five test targets, one zip (header fields, already preceded
by native/expected length equality), seven nested-vector probes feeding complete
native-value equality, six whole constant-table comparisons, and two collection
helper output paths. Scalar signing/preimage outputs use whole-string equality.
That is a census, not a claim that inspection alone proves mutation sensitivity.

The 16 production collection sites exercised individually are:

- BASE_URLS, WSS_URLS, ENDPOINTS, INSTRUCTIONS, GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC.
- collectAuthParams object output and batch-array output.
- BackpackDepth.bids and .asks; BackpackCollateral.collateral;
  BackpackWsSubscribeMessage.params; BackpackWsUnsubscribeMessage.params;
  BackpackWsDepthUpdate.b and .a.
- generateAuthHeaders header vector.

There are no implemented REST/WS handlers needing handler-specific field
inventories yet. Those rows remain pending. Record probes explicitly name their
fields and compare complete native objects against source observations; they
are not arbitrary handler-output patch comparators.

## Guard layers

Source counts are independent constants in Rust, not values reported by the
oracle under test. Six tables have fixed per-table entry counts. Types pin
1545 cases, per-record accepted/refused counts across29 observation groups
(28 interfaces plus retained generic-data observation), and318 accepted plus
1227 refused comparisons on each of three numeric ingress routes. The954 queued
wire comparisons and954 completed wire assertions are separately checked.

API source groups pin101 window,30 classification,900 collection,172 public URL,
1548 instruction,2 UTF-16 string and3 UTF-16 data cases. Signing pins41 query,
2132 payload and2 UTF-16 cases. Auth pins198 signatures/63 successes and264
headers/80 successes, with five source fields per accepted header case and400
completed native header-field comparisons. Completed-loop counters are asserted
separately from source admission.

Nested native arrays and objects receive independent length checks before any
indexed recursion. Full native-value equality still follows, checking scalar
values, numeric bits, types and keys. The header zip retains its independent
length check. Counts do not replace field-identity assertions.

All TypeScript files, oracle files, schema fixtures and production source remain
byte-identical. No fixture-value repair, regenerated source artifact, per-case
Node spawn or concurrent Cargo invocation was used. Each test invocation executes
one Node process for its whole corpus. All Cargo controls use one scoped test
and jobs1.

## Boundaries on the evidence

Source-subcorpus removal controls specifically target types.cases,
API.collect, signing.payload and auth.headers. Other fixed subgroup/record
counts were inspected; this does not relabel them as individually mutated.
The two original API subset drafts first failed on singleton-to-empty inputs;
the final controls only mutate lengths above1 and prove2-to1 failures at cases21
(object) and15 (batch). Seven record-vector full cases also contain two elements,
so removing one retains a nonempty subset. Compile-only draft controls were
rejected, repaired, and excluded from final behavioral-control counts.

Extractor failures prevent a fully green CI run. The merge freeze is the user's
policy, not GitHub branch protection. The installed local extractor additionally
fails to load smol-toml/index.js; no extractor pass or overall merge-readiness
claim is made by this audit.

## Recorded controls for shapes1–6

The final recorded set for these six shapes contains91 control/restore pairs:
8 baseline source-corpus reductions incorrectly passed; the other83 controls
failed named runtime assertions at exit101. Every restoration exited0.
The eight baseline passes demonstrate a test gap, not a production defect.

| Control set | Cases | Outcome / named assertion family |
| --- | ---: | --- |
| Original production empty/subset, all16 collection sites |32|All rejected by existing full-value/native/length assertions|
| Original source-corpus empty/subset, four targets |8|Incorrectly passed; coverage gap confirmed|
| Same source-corpus reductions after added guards |8|TYPES/API/SIGNING/AUTH-CARDINALITY source counts|
| Same reductions with source guards disabled |8|TYPES/API/SIGNING/AUTH-CARDINALITY completed comparisons|
| Extra native outputs, all16 sites, source guards disabled |16|TABLE/NATIVE-CARDINALITY or header census|
| Empty/nonempty-subset native outputs with source guards disabled, five representative paths |10|TABLE/NATIVE-CARDINALITY or header census|
| Expected per-case rows removed/shortened, four targets, source guards disabled |8|TABLE/NATIVE-CARDINALITY or header census|
| Both source/native header fields empty, source guards disabled |1|AUTH-CARDINALITY completed comparisons: [198,264,0] versus [198,264,400]|

The five representative bypass paths are BASE_URLS, object auth collection,
batch auth collection, BackpackDepth.bids, and the header vector. They are not
reported as a second individual bypass run for every one of the16 sites.
Expected-row controls cover BASE_URLS entries, BackpackDepth's native field map,
collection-helper row/field arrays, and header fields. In particular, keeping
case counts intact while making an expected field map empty does not pass.

The guarded header zip rejects short, empty and extra native vectors. Equal
lengths at zero are caught by its independently fixed400 completed-field count.
Oracle-driven outer loops reject removed cases first at source admission and,
when that is bypassed, at independently fixed completed-comparison counts.
Full native-array comparisons and explicit length checks reject surplus output;
no indexed walk silently ignores a native tail.

Logs and machine-readable ledgers are retained in
`sdk-rs/target/backpack-cardinality/`: `results.json`,
`baseline-source-results.json`, `guarded-source-results.json`,
`bypass-source-results.json`, `native-bypass-results.json`, and
`expected-rows-results.json`. Sub-agent inspection read all182 corresponding
logs and found no assertion/exit/restore mismatches.

## Consuming sequence controls (shape7)

Permanent runner: `tests/consuming_controls.py`, invoked with
`SAFE_CEX_TYPESCRIPT_PATH` and optionally `CARGO`; its
`--without-cardinality-guards` mode runs the companion control. It temporarily
adds a field-level Serde decoder to each of the seven concrete wire-vector
fields listed above. The injected SeqAccess::next_element_seed drains the
underlying input using IgnoredAny until exhaustion and THEN returns None to the
Vec deserializer. This is not a non-consuming early return and does not rely on
Serde rejecting trailing elements. No shared numeric deserializer is modified.

All seven consuming mutants compile and fail named NATIVE-CARDINALITY assertions,
then restore and pass. The companion keeps the same production consuming mutant
but removes every newly added cardinality assertion and the recursive shape
walk. All seven still fail the original `decoded native fields/bits ...`
assertions, then restore and pass. This is already-covered consuming-loss
sensitivity from full native-value equality, not a newly discovered production
bug or a claim that the new length check is its sole detector. The independently
confirmed false green was removal of entire oracle cases/groups.

These14 control/restore pairs are recorded in consuming-guards-on-results.json
and consuming-guards-off-results.json, with28 corresponding logs. Combined with
the preceding91 controls:105 final recorded pairs,97 named runtime failures,
8 baseline corpus-loss false greens, and105 passing restorations. Inspection of
seven owned concrete Vec fields is the census; these14 consuming runs are the
proof. Arbitrary caller-defined generic API-response data is not relabelled as
seven additional owned decoders.

After restoration, integration a09ea6489b2012e43dfca621621ae8e916d4d03c was merged
at2497894c644ea5e939c8dc88def14dc5fc04f1f9 (two chaser manifests,16 added lines).
The integration extractor manifest repair is inherited. The user separately
reported six KuCoin unused-Result compile errors on that integration head;
Backpack-scoped checks below must not be described as workspace success.

## Final scoped checks and independent review

After the a09 integration merge and all restorations, full Backpack suite:
5 tests passed,0 failed. Strict host and wasm Clippy (all targets, jobs1) passed
as separate invocations. Scoped fmt and whitespace checks passed. Official
scoped parity-check indexed exact restored Backpack source plus its shared
Ed25519 mapping:147 rows /76 public Rust items /0 errors. Python3.14 tomllib:
8 ported /36 divergent /103 pending. Shared exemptions359/359; no edits.

Post-a09 cutover inventory iteration1:0 implemented Backpack Store writes and
0 public-data subscriber callbacks. Locked metadata confirms3 authorized direct
workspace edges and runtime closure Backpack,num,types,crypto (no Store).
A sub-agent independently rechecked that callgraph and closure. This finite
implemented-callgraph result is not a claim about future REST/WS adapters or
arbitrary caller-provided code.

Sub-agent review read all210 logs across the105 final recorded controls, checked
the exact consuming adapter and seven-field inventory, and reported0 actionable
findings. The guards-off companion demonstrates independent old native-value
protection, not that removing a new length check makes these seven targets pass.
Exact committed-SHA review follows the evidence commit.

Local extractor module loading was retried after a09 and still fails with
ERR_MODULE_NOT_FOUND for the existing installation's smol-toml/index.js. This
local environment failure is distinct from the two repaired integration
manifest defects. No extractor success, workspace build success, fully green
CI, or overall merge-readiness is claimed. No CI rerun or push intended to
trigger CI was performed. Follow-on delivery uses a separate branch; the
original reviewed PR head is unchanged.
