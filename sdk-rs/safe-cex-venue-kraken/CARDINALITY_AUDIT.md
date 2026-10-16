# Kraken cardinality audit

Scope: the four implemented witness targets `types`, `utilities`, `helpers`, and
`reads`. Pending venue methods and the stashed leverage read are excluded. This
is an audit of those targets, not a whole-venue coverage claim. Integration
`a09ea6489b2012e43dfca621621ae8e916d4d03c` is merged into the follow-on branch;
PR 7270 remains pinned at `0bbfaca058aa514284efe750dd072b78cd9dd0a6`.

## Measured original behavior

The sole Rust zip (URL enum variants) follows complete-array equality. Collection
mappers and positions compare complete captured native arrays, not paired prefixes.
Fourteen production controls compiled and exited 101 at existing named assertions,
then restored byte-exact and passed: empty and subset outputs for fill batches,
income rows, income batches, account-log reads, Spot positions, the futures side
table, and the futures public endpoint array. These are seven concrete sites;
they do not establish that every constant table was independently mutated.

A separate **real false green** exists in the original token target: conditional
array-token `clear()` and `pop()` mutations each compiled and passed the original
read target (4 passed, 0 failed). Its only array token was empty. The production
file was restored and the original target passed. This is a missing nonempty
collection case, not an unguarded zip.

## Strengthening

`tests/support/cardinality.json` pins source case counts, source total counts,
per-case array-output cardinalities, all 21 type export lengths, five nested WS
table lengths, type lookup probe counts, and ID-call counts.
It contains counts only, not replacement expected output values. Four top-level
corpora now use independent committed bounds. Nested ID schedules execute fixed
`0..1320` loops, with per-scenario 1320 and total 2640 native comparison assertions.
Lookup probes also use independent bounds. Complete-array value assertions remain.
Native vector lengths are observed before capture and compared independently
against live source output lengths.

The read supplement adds two actual-source token arrays (lengths four and two,
including a nested two-element array) in the same Node process. Its VM executes
the original oracle file unchanged and invokes its actual-TypeScript token runner.
The original token corpus remains 16 cases; the supplement adds two. Source
account and position cases remain 20 and three, with position output counts
`[0, 4, 6]`. Helpers retain two ID scenarios of 1320 calls each.

Original oracle files preserved byte-identically:

| Oracle | Bytes | SHA256 |
| --- | ---: | --- |
| types | 4464 | 74a5345449086a2312afb77a37e38b16bd0ccba5492737c70a3117fb550a17bb |
| utilities | 9544 | e87f2fb06e93d165b77cc79259e17a6f03a3787588f7ad079a20458508ca67cb |
| helpers | 7567 | d886f9d328d8e9d6b1a9a34774ceb65f55a63e95e20200bf481e5edada519cac |
| reads | 7734 | 70da2916f8efa6626a1f47f74e23d30432a648e6823b2593d8d16e946959c8fa |

## Reproduction and evidence scope

Set `SAFE_CEX_TYPESCRIPT_PATH` to the installed TypeScript package and run
`python3.12 sdk-rs/safe-cex-venue-kraken/scripts/cardinality-controls.py`.
Each control uses one scoped Cargo test target, `--jobs 1`, and one Node process
for that entire corpus. The runner requires exit 101, a named runtime assertion,
and `test result: FAILED`; a compile failure never counts. Each mutation restores
original bytes in `finally` and requires a subsequent passing target.

The runner contains 63 controls. The initial 14 prove the existing full-array
assertions; 16 strengthen source/case/index guards; 28 extend the proof to the six
shapes: 16 production empty/subset mutations with source guards bypassed, eight
extra native outputs, and four source export/nested-table omissions. Five direct
zip-witness controls shorten/enlarge the native enum list or truncate the source
list with census bypassed. These five mutate test enumerations, not production
enum definitions. The source
census asserts its numbers; nothing relies on a printed count.

The additional controls cover both array-token mutations, empty/subset source
corpora in all four targets, empty/subset nested ID schedules, those nested
mutations with the source census bypassed, and native position loss with the
source census bypassed. A first bypass attempt caused an unused-function compile
error and was rejected as evidence; the corrected control references the census
function without invoking it, preserving compilation while bypassing the guard.

No manifest row is promoted by this audit. The measured triple remains
24 ported / 17 divergent / 92 pending (133 rows). No CI result or workspace build
is inferred from these scoped gates.

The six-shape proof is targeted at eight concrete collection sites. Scalar helper
results use complete-value equality; their source loops and nested ID schedule
have separate omission controls. No handler-specific field inventory is claimed:
Kraken websocket handlers remain pending. The one zip has an independent two-case
source pin, native length assertion and complete-array equality before iteration.
Native overproduction is rejected by independent length checks and whole outputs.

Integration `a09ea6489b` includes both extractor manifest repairs; the inherited
extractor-red caveat is retired. The integration merge freeze remains the owner's
policy, not branch-protection enforcement. Local scoped results are not a PR CI
result.


## Consuming decoder controls (shape seven)

This is injected-fault evidence, not a claim of a live decoder bug. Earlier row
removal controls acted after decode; they did not establish this property.

Census: two explicit `decode::<Vec<JsNumberInput>>` sites (fills and income batch)
and one recursive `Wire::Array(Vec<Wire>)` shape. The focused proof exercises four
mapped paths: fill batch, income batch, account-log read and token array, plus a
nested token array. This is not an inventory/proof of every possible recursive
JSON instance or pending venue method.

`scripts/consuming-controls.py` temporarily changes shared
`safe-cex-num/src/numeric_ingress.rs::Array::next_element_seed` inside this
worktree. It consumes the entire iterator before returning None, or consumes the
remainder after retaining one element. Serde's trailing-element check therefore
cannot explain the negative result. No shared production change is committed.

Measured: four mutation configurations, seven failing scoped target runs, thirteen
named path assertion failures and seven restored passing target runs. The empty
and subset configurations each fail independent native lengths on all four paths:
2 fills, 5 income records, 8 account-log records and 4 token elements. The nested
configuration fails `KRAKEN-CONSUMING token nested native length` (expected 2).

The second-layer configuration keeps the consuming-empty mutant active and removes
both outer length checks per focused test. All four paths still fail their named
`KRAKEN-CONSUMING ... native index coverage` assertions. Fixed index bounds and
explicit completed-comparison counts prevent an empty result from skipping the
comparisons. This reports a remaining independent guard, not a false claim that
the length checks are the sole defence. The previous original target was not
measured under this consuming fault; no retrospective coverage verdict is claimed.

The shared source is restored byte-exact to SHA256
`629cedca9848b4d431b30b0f4d96e0a907297d2452472c707b9c593bd94dbcb3`.
The four original oracle files and their existing inputs remain unchanged. Both
focused tests in each target share that target's one Node corpus invocation.
