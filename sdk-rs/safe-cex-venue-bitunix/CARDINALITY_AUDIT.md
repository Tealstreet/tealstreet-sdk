# Cardinality audit

Baseline code: `7cbd3a1170f0ff3f445adde1a9c2763dfee62522`.

Read all five Rust witness targets. None uses zip to compare outputs. The wire
witness compares complete projected native arrays, and constants compares whole
JSON values with an export-key census. The three scalar utility targets assert
source corpus sizes (25, 25, 225), then compare one native result per input;
string/code-unit comparisons compare complete values. No collection-producing
formatter or REST reader is implemented yet.

Measured controls against the ORIGINAL targets:

| Mutation | Compiles | Exit | Interpretation |
| --- | --- | --- | --- |
| Production response decoder clears positions | yes | 101 | Existing decoded-fields equality caught empty output |
| Production response decoder truncates positions to one | yes | 0 | REAL FALSE GREEN: every accepted original case held one row |
| Oracle drops all position cases and interval literals | yes | 0 | REAL FALSE GREEN: source-loop cardinality unasserted |
| Production ORDER_SIDE becomes empty | yes | 101 | Existing constant witness ORDER_SIDE caught omission |
| Production ORDER_SIDE drops SELL | yes | 101 | Existing constant witness ORDER_SIDE caught subset |

The wire gap is not a zip in this venue: complete-array equality already rejects
short output for any particular nonempty expected array. The blind spots were
single-row-only positive cases and an unguarded source loop. Do not generalize
the ORDER_SIDE mutation into a claim that every constant was mutated this way.

Repair: a separately labelled source supplement adds empty and three-distinct-row
response cases, executing the existing actual-TS typed capture in the same Node
process. The original wire oracle has zero fixture-value repairs and remains
byte-identical, SHA256
`52134d2bf0031fa8e0fce51d31d21b5469ab930c7c6cd9269558ad743c72dae5`.

Independent layers now assert:

- Source census: 103 original cases, 30 interval literals, 20 fields, 2 supplements.
- Each accepted case's expected output length equals its raw input array length.
- Native decoded length independently equals expected output length on each route.
- Totals: 40 scalar-compatible source rows, 160 actual native row comparisons,
  412 route checks and 2 explicit surrogate-refusal cases.

`tests/cardinality-controls.py` requires EXIT 101 after successful compilation
and a named assertion, then restores and requires EXIT 0 for all eight controls:
production empty/subset, source empty/subset, source output subset, source output
subset with its source length assertion BYPASSED, and ORDER_SIDE empty/subset.
The bypass must fail `wire native response count`; it cannot be satisfied by
faking the first layer. Logs live under target/bitunix-cardinality, including the
five baseline logs. No shared fixture is changed. Both published heads stay pinned.

## Six-shape and consuming-decoder follow-up

Census: all five witness targets were inspected. The implemented collection
response is GetPendingPositionsResponse, exercised through four decode routes.
The constants target compares whole tables; the three utility targets produce
scalar results. Scalar corpus mutations below are test-population controls, not
claims that a scalar production function can return a collection.

At baseline b684da279170ed5ee1a094e4cdcc152e462ce25c, five deliberately bypassed
primary guards exposed missing independent comparison counts: SHA with only four
refusals, ID helpers with only two refusals, value helpers with only five refusals,
constants with no exports, and intervals with no literals all passed. These are
measured guard-bypass gaps, not unmodified-target passes. Logs are retained in
`target/bitunix-cardinality-six/*-baseline.log`.

The additional independent counts are now 12 constant exports; 21 SHA native
comparisons and four refusals; 23 comparisons for each of the three ID
observations; 220 comparisons for each of the three value helpers; 60 positive
interval decodes, 30 interval serializations, and 14 invalid-interval checks.
Per-helper counts prevent one populated helper from masking another. Existing
source totals, whole-value comparisons, and response count layers remain.

Measured proof (`tests/cardinality-six-controls.py`, 20 compiling controls):

- Each scalar corpus is emptied, shortened, extended, and reduced to refusals,
  with its primary source count bypassed. Each fails its native comparison count.
- Production response outputs are emptied, shortened, and extended with both
  source guard blocks bypassed. Each fails `wire native response count`.
- A shortened expected response with those guards bypassed fails the same native
  length assertion. Empty interval and supplement groups fail their source counts.
- Removing expected position fields fails whole decoded-field equality.
- An extra production ORDER_SIDE entry fails `constant witness ORDER_SIDE`.

Measured proof (`tests/cardinality-seven-controls.py`, eight compiling controls):

- Constants and interval groups are each emptied, shortened, and extended with
  primary guards bypassed. These fail `constant actual export comparison count`
  and `wire native interval decode count`, respectively.
- An injected production SeqAccess drains every input element through IgnoredAny
  before returning None. It compiles and fails `wire native response count`.
- Keeping that consuming mutant while removing both the source per-case and
  native length guards still fails `position decoded fields`. Complete-vector
  equality independently detects empty output; this is not a claim that length
  checks are its sole protection.

Every control requires successful compilation, exit 101 and the named assertion,
then restoration with exit 0. There are 40 six-shape and 16 seven-shape failed/
restored logs. The consuming control fails on the first raw decode route; the
four-route census is not four separately isolated consuming-mutation proofs.
This is an injected mutation audit, not a discovered live production decoder bug.
No TypeScript source or oracle fixture values were rewritten. The original wire
oracle hash above remains unchanged. Production source is restored after controls.
