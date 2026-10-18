# Coinbase type/table gate evidence

Baseline: `6922766a6c`, branch `feat/rust-venue-coinbase`.
Measured baseline: 426 pending, 0 ported, 0 divergent across 16 manifests.
This slice: 394 pending, 32 ported, 0 divergent.

All commands run inside this worktree with `/opt/homebrew/opt/rustup/bin` on PATH.
Yarn commands use `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js`
locally to avoid the Volta exit-code defect. CI uses corepack.

- `cargo test --locked -p safe-cex-venue-coinbase --all-targets -- --include-ignored`:
  33 passed. The oracle executes the current TS table exports and inspects all
  declared DTO fields/literals. It rejects unsupported AST shapes rather than
  silently skipping them. No network or signing is involved.
- Rust endpoint mutation: change MARKETS `/products` to `/MUTATED` in `tables.rs`;
  `table_endpoints` fails with `ENDPOINTS`, exit 101. Restore: exit 0.
- Separate TS endpoint mutation: same edit in `coinbase.types.ts`;
  same named failure, exit 101. Restore: exit 0.
- Rust literal mutation: change the `StopDirection` serde spelling;
  `wire_stop_direction` fails, exit 101.
- Separate TS literal mutation: change `STOP_DIRECTION_STOP_UP` in the TS union;
  `wire_stop_direction` fails, exit 101. Both files restored byte-for-byte.
- Scoped `cargo fmt --check`: pass. Malformed-format private function: exit 1;
  restored file passes.
- Scoped plain `cargo clippy --locked --all-targets`: pass. Add unused private
  `unused_coinbase_negative_control`: exit 101 through workspace-denied warnings;
  restore and pass. No lint relaxation or extra `-D warnings`.
- Scoped `cargo build --locked --target wasm32-unknown-unknown`: pass. Add
  wasm-only `compile_error!`: exit 101, naming the control; restore and pass.
- `parity:check`: passes, 6,901 rows / 1,088 public Rust items. Change RECV_WINDOW
  mapping to NONEXISTENT: exit 1 with correspondence, resolution, and invention
  failures; restore and pass.
- `node --test scripts/rust-ci-wiring.test.mjs`: 51 pass, including layer policy
  and negative controls removing, masking, or conditioning the Coinbase step.
- `parity:extract --check`: passes with 0 changed / 0 added / 0 orphaned after
  the prescribed worktree-local frozen Yarn install repaired missing smol-toml.
- `parity:report` and `parity:progress`: both exit 0; reports are inventories,
  not additional behavioral assertions.

The first test run exposed default Serde struct acceptance of positional arrays,
including empty arrays for all-optional structs. The JSON-object adapter now
rejects those inputs without weakening the oracle. TS declarations describe
finite JSON DTOs; this slice does not claim to preserve arbitrary extra keys,
undefined object members, non-finite JS numbers, or object identity.

No exchange, socket, placement, or Starbase runtime behavior is claimed yet.

## Adversarial review repair

The isolated reviewer reproduced three silent passes at `411ccf1fb6`: deleting
optional TS `cursor`, narrowing TS `STOP_DIRECTION`, and prepending a duplicate
Rust MARKETS endpoint. All three now fail their named Cargo tests at exit 101.
The first two are caught by a complete TS/Rust AST schema comparison; the third
by comparing ordered entries directly without map deduplication. Restored
baseline passes. The added `syn` dependency is test-only. String samples carry
field-specific paths to expose accidental same-type field swaps.

Coverage negative control: adding exported COINBASE_COVERAGE_NEGATIVE_CONTROL to
the TS source caused exit 1 naming the missing symbol; restored check exited 0
with 0 changed manifests. Isolated re-review of `6f929e964d` repeated all three
adversarial mutations at exit 101 and restored 33 tests at exit 0: MERGE verdict.

## Starbase protocol constants

Twelve exported constants across the inert Starbase family have their own live
TS oracle, preserving ordered object entries and every array member. Thirteen
protocol tests pass (46 crate tests total). Independent HTTP endpoint mutations
in Rust and TypeScript each fail `protocol_coinbase_starbase_endpoints` at exit
101. Counts after this slice: 382 pending / 44 ported / 0 divergent. Strict
Clippy, scoped wasm build, fmt and parity invention/status pass. No activation
flag is enabled; all source false literals remain false.


## Alias and order formatting helpers

Counts: 377 pending / 48 ported / 1 divergent. Three differential tests cover
21 product IDs, 1,344 market maps (including null/false/zero/empty-string/array
and object precedence), and known/unknown state and order-type switches with
exact console.error callbacks. The inherited-key defect is CORRECTED on the
resolver row, with five prototype names and 40 absent-key cases pinned to TS.
Independent EURC exception mutations in Rust and TS fail
`usd_equivalent_aliases` at exit 101; restoration passes all 49 crate tests.
Scoped fmt, plain strict Clippy, wasm and parity invention/status pass.

Isolated protocol review of `a388d6a1b4`: MERGE. The reviewer independently
proved endpoint changes on both sides, duplicate Rust endpoints, TS resolution
reordering, new TS table exports, and activation-flag mutations fail at 101;
restored 46 tests pass. No runtime activation or live venue requests occurred.


## CFM sizing and REST orders

The central Number(string) export `a26c0ac381` is incorporated as `d340225227`,
unchanged apart from preserving the existing neighboring exemption rationale
when resolving the additive manifest conflict. No CI gate steps changed.
Counts: 372 pending / 52 ported / 2 divergent. The maintained TS oracle covers
27 CFM numeric values and 362 REST orders, including all eight configurations,
base-versus-quote sizing, fill guards, CFM multipliers, invalid numeric values,
prototype status keys and retained raw fields. All 51 crate tests pass; fmt,
strict Clippy, wasm and parity invention/status pass.

Independent Rust/TS mutations of the fill comparison (> to >=) fail the named
REST-order test at exit 101. Independent Rust/TS mutations accepting zero CFM
sizes fail the named CFM test at exit 101. Restored tests pass.

The mapper records two precise differences: CORRECTED inherited status keys
cannot inhabit OrderStatus; CHOSEN shared Order serde layout emits extra last.
A custom venue wrapper could preserve TS layout, so the original FORCED label
was too strong; the shared Store-facing return type is the reason for this choice.
Tests retain ordered raw info comparison and adjust only that declared top-level
field-order difference. This is a typed domain projection, not signed JSON.

Isolated review of `9c94e4b010`: MERGE after both-side truthy/nullish and console
message mutation probes at exit 101, restored 49 tests at exit 0.

Isolated review of `e350b548d4`: MERGE. Expanded numeric probes, raw-info,
stop-price, fill and Number/parseFloat mutations failed by name at 101; restored
51 venue tests and the central decoder export test passed.

## Starbase session contracts

Measured 357 pending / 67 ported / 2 divergent. Four maintained tests cover
four declaration schemas, 45 requests, 15 mutation results, 916 get results,
and four error strings including U+FEFF. Strict extra-field/error precedence
includes keys sorting before, between, and after the required keys.
Independent Rust and TS length-guard mutations fail
`session_decoders_exact_keys_and_error_precedence` at exit 101. A Rust scope
literal change and TS optional-to-required field change independently fail
`session_declared_shapes_match_both_directions` at exit 101. All restored
55 crate tests pass. fmt, plain strict Clippy and scoped wasm pass.
Extract reports zero changed/added/orphaned rows; parity check/report/progress
pass. The first parity-check invocation lacked cargo on PATH (exit 127);
it was rerun successfully with the required rustup PATH.

The domain is finite JSON plus root missing/undefined. JS object properties
owning undefined are normalized at the JSON-RPC boundary before comparison.
Protocol errors retain name/message and Error.toString; host stack traces are
diagnostic metadata. No transport, signing, or runtime activation is added.

The first isolated session review held `65bcbaedc2`: removing Rust generic
bounds or narrowing the TS Method bound incorrectly passed. The schema gate
now compares parameter count/order, names, constraints, and defaults; unsupported
constraint syntax fails closed. Both reviewer mutations now independently fail
`session_declared_shapes_match_both_directions` at exit 101, then restore green.

Isolated re-review of `8a000c4786`: MERGE. Removing Rust bounds, narrowing TS
method/record constraints, and adding a Rust default each failed the named
schema test at 101; restored 55 tests passed.

## Starbase protocol validation

Measured 352 pending / 72 ported / 2 divergent. Added live comparisons for
830 method inputs (ASCII grammar plus U+FEFF, NEL, line terminators and astral
characters), 19 record/ID values, and 1,482 error objects. Optional unknown data
retains null while missing data is absent. Three new tests bring the crate to
58 passing tests; declaration comparison covers every error-object field.
Separate Rust and TS method-grammar mutations fail `starbase_method_ascii_grammar`
at 101. Separate Rust upper-ID and TS lower-ID mutations fail
`starbase_record_safe_id_and_rpc_error_validation` at 101. All restored tests,
fmt, strict Clippy, scoped wasm and parity invention/status pass.

Isolated review of `de0c12f90e`: MERGE. Both-side mutations dropping null error
data and restricting integer codes to safe integers failed at named101;
restored 58 tests passed.

## Cutover declarations and blocker collectors

Measured 342 pending / 82 ported / 2 divergent. Eight live type schemas,
384 recursive decode/serialize cases, and all 2,048 valid evidence combinations
across three product classes are compared. Ten new tests bring the crate to
68 passing tests. Independent Rust/TS implementation-status literal mutations
fail `cutover_implementation_status` at 101; a Rust false-only decoder mutation
fails `cutover_plan` at 101. Removing the permanent WebSocket blocker separately
in Rust and TS fails `cutover_blocker_order_for_every_evidence_combination` at
101. Restored suite, fmt, strict Clippy and scoped wasm pass. Parity check
initially rejected incomplete anonymous-type rename notes; explicit target names
were added but the follow-up still failed: the gate requires fully qualified
paths, not bare type names. The earlier success wording was premature. Full
paths and adapted layers are now supplied; results are recorded after rerun.

Evidence validation and selectors remain pending. JS own-key enumeration is
being factored centrally. The pending validator row records the TS empty-string
unknown-key truthiness bug, which must be retained, not corrected.
The corrected fully qualified mappings pass parity-check: 6,901 rows and
1,156 public Rust items, exit 0. No implementation change was needed.

## Accepted-shape audit and union repair

Measured counts stay 342 pending / 82 ported / 2 divergent. The audit found
41 actual port defects on already-ported claims: every one of the 21 legacy
and 20 cutover string unions accepted tagged objects alongside strings. The
full-sweep rejection test listed all 41 and exited 101 before repair. Each now
deserializes String before a private function-local derived Wire enum. No
remote Self or public permissive conversion path is added. The false-only
singleton was already strict; session scope has no Deserialize implementation.

Maintained TS accepted-set inventories cover all 42 deserializable enums; actual
Serde metadata comes from deserialize_enum or the derive's unknown_variant
list behind the string gate. Tagged objects, wrong scalars, arrays, case variants,
BOM/NEL and other invalid strings are rejection probes. Injected aliases in
both files, a skipped Rust variant, and an added TS literal fail the named
inventory tests at 101. An injected public permissive StopDirection::deserialize
passes the trait path but fails the concrete qualified-call regression at 101.

The expanded struct audit found zero widenings across all 107 legacy and nine
cutover decoded records: map-only gates were present already. It now tests
complete positional arrays through trait and concrete qualified calls for every
record, plus inventory completeness in both directions. Removing a map guard
in either file fails its named record test at 101. Injecting an inherent public
array-accepting record decoder also fails the qualified-call regression at 101.
The five other protocol/session records are serialize-only, with explicit
function validation; TryFrom implementations accept Map, not arrays.

The requested tree-wide lift (including structs and alternate public decoders)
was estimated above the one-hour limit and reported for central routing. The
review identified real callable-path, manual-decoder, configuration and legitimate
object-union classification requirements; no static-only substitute was claimed
as runtime proof. The local generic probes are available for the central owner.
Restored acceptance repair: all 74 crate tests, fmt, plain strict Clippy, scoped
wasm and parity-check pass. The injected public permissive enum decoder fails
`qualified_public_decoders_reject_tagged_objects` at exit 101. The expanded
session corpus also rejects tagged scope objects without adding Deserialize.

## Alternate decoder review control

The adversarial review held delivery: adding FromStr accepting an extra alias
passed the 74 runtime tests and parity-check, whose public-item scanner skipped
trait implementations. This was an injected bypass, not a surviving production
decoder. The new decoder_boundaries test inventories explicit implementations
recursively across crate source files and classifies derives, manual decoders,
serialize-only types and the two outbound conversions. Unreviewed traits and
inherent methods fail closed. Runtime probes remain necessary alongside this
source inventory; neither proves the other.

FromStr, TryFrom<Value> and alias-accepting inherent decoder controls each fail
`every_public_wire_decoder_entrypoint_is_classified` at exit 101. A qualified
cross-module FromStr owner fails the same test at 101. All controls restored;
75 crate tests and strict Clippy pass. Counts remain 342 pending / 82 ported /
2 divergent. Central gate ownership has transferred to codex-f5ojf1.

## Shared coercion merge

Merged feat/rust-p4-utils at 5cf6b4d8d8. Both parents had 55 named CI
steps; the combined workflow has 56, retaining every name and multiplicity
from both parents. Conflicts resolved per hunk: retain both sets of mandatory
CI controls, preserve numeric spelling rationale while delegating to the shared
backend, and retain all additive exemptions. CI wiring audit passes 53 tests;
Coinbase passes 75 tests; parity-check passes 6,901 rows / 1,282 public items.
The first CI audit attempt failed because Cargo was absent from PATH; rerunning
with the prescribed Rust path passed. Shared ownership-order primitive is not
in this merge, so the evidence validator remains pending.

## Starbase authentication result

340 pending / 84 ported / 2 divergent. AuthResult carries the exact bearer
singleton; it and its token enum are serialize-only, with the explicit source
validator owning unknown input validation. Live TS schema comparison and 126
input cases cover missing fields, tagged token objects, complete positional
arrays, all wrong primitive shapes, empty/whitespace/BOM/NEL token/scope values,
fractional/zero/negative expiration and integers beyond the safe-integer bound.
Independent Rust and TS mutations accepting zero expiration each fail
`starbase_auth_declaration_and_validation` at exit 101. Restored: 76 crate tests,
fmt, plain strict Clippy, wasm check and parity-check pass. Extractor found two
noncanonical array whitespace spellings in the earlier cutover mapping; its
canonical formatting was committed separately. Parity report/progress ran.

## Transport and heartbeat declarations

336 pending / 88 ported / 2 divergent. Four more declarations are serialize-only:
transport kind/context, heartbeat signal and test result.
The live AST oracle expands keyof endpoint keys and nested type references,
compares every field and literal, and pins omitted optional values versus present
empty strings. Decoder classification rejects adding unreviewed decoding
implementations; class-level input validation remains pending separately.
Independent Rust/TS heartbeat spelling mutations and optional-field mutations
each fail `starbase_transport_and_heartbeat_declarations` at exit 101. Restored
77 tests, plain strict Clippy and scoped wasm pass. Parity initially rejected
client/request options because private TS aliases have no independent rows; those
options are deferred to their owning classes. The preceding
auth slice received MERGE after independent trimming and safe-integer mutations
each failed at 101.

Final four-declaration scope passes parity-check: 6,901 rows / 1,289 public items.
The declaration review found an oracle gap: skipping the required context endpoint
in serialization still passed because only authorization was compared. Complete
context comparisons now cover every key for both live-TS transport spellings.
Independent endpoint and kind omission controls each fail the named declaration
test at exit 101; restored Starbase tests and plain strict Clippy pass.

## Credential precedence and base64 boundary

334 pending / 90 ported / 2 divergent. Added getCoinbaseJsonSecret and
base64ToBytes against actual TS declarations: 341 alias cases and 6,408 base64
cases. Empty strings retain alias priority. Prefix removal happens before
whitespace stripping; the implementation uses shared JS whitespace splitting and
the shared browser-compatible base64 codec with explicit source padding.
Malformed padding, nonzero trailing bits, URL alphabet, case, whitespace, BOM/NEL
and decoded lengths 0 through 65 are covered. Unicode scalar strings are the
Rust boundary; the initial corpus accidentally split an astral code point into
lone surrogates, which serde_json cannot decode, and was changed to insert
whitespace between code points while retaining the astral invalid-input case.

Replacing JS whitespace with Rust whitespace and dropping empty secret aliases
each fail their named tests at exit 101. Separate TS whitespace/empty-alias
mutations fail those same named tests at 101. Restored 79 tests, plain strict
Clippy, wasm and parity-check pass (6,901 rows / 1,291 public items). The earlier
context repair received MERGE after independent endpoint/kind omission controls.

## Public-data literals and validators

321 pending / 103 ported / 2 divergent. Five serialize-only literal enums and
eight transport-free guards now correspond. Live TS compares all literal
declarations and serialized values; 864 general guard cases, 1,440 minimum
cases and six protocol-error contexts pin exact results/errors. Finite JSON
values compare as binary64 (not arbitrary-precision JSON lexical spelling);
diagnostic strings still require the shared ECMAScript numeric spelling.

The NaN-minimum behavior is recorded on requireFinite: TS rejects on value <
minimum, which is false for NaN. The superficially equivalent negated >= check
is deliberately rejected by the oracle. Independent Rust/TS controls for that
comparison, interval spelling and whitespace trimming each fail their named
tests at exit 101. Restored: 83 tests, fmt, plain strict Clippy, scoped wasm,
parity-check (6,901 rows / 1,304 public items), and zero extractor drift.
Credential review returned MERGE after 16,461 extra independent base64 cases
and separate Rust/TS prefix-case and empty-alias controls.
The public literal review held delivery: numeric index narrowing and removal of
as-const changed the TS type without changing table values, while the oracle
ignored both. The oracle now requires a number index, literal as-const array,
and no unclassified type annotation. Index [0], removed as-const, and explicit
readonly string[] annotation controls each fail the named declaration test at
101. Restored public tests and plain strict Clippy pass; counts remain 321/103/2.

The panic-rule sweep found three redundant invariant expects (checked-number
extraction, guarded object extraction and infallible unit-enum serialization),
not a demonstrated malformed-input panic. They are removed; no panic/unwrap/
expect/assert remains in venue production source. Session outbound Map conversion
now constructs its two string values directly and compares the complete Map to
live TS builder params. An account-to-connection outbound mutation fails the
named session builder test at 101. Restored session/public tests and Clippy pass.
Counts unchanged; no new divergent finding was claimed without a panic witness.

## Public state, currency and symbol parsers

316 pending / 108 ported / 2 divergent. Five explicit parsers now reject tagged
objects, wrong scalars, case changes, whitespace and non-ASCII name grammar.
The live source contributes 3,515 name/parser cases, pinning distinct unknown-base
versus malformed-name errors and exact legacy/current symbol conversion. No
regex engine is used. Independent Rust/TS added-currency and malformed legacy
suffix controls fail the named parser test at 101. Restored 84 tests, Clippy,
wasm and parity-check pass (6,901 rows / 1,309 public items). Cargo work is now
serial with reviews and capped at two jobs per the machine-resource directive.
The tuple oracle repair and expect cleanup each received MERGE.

The tuple-inference repair now covers all sibling oracles via literal-tuples.cjs.
Review independently confirmed session/cutover lost-as-const mutations previously
passed at exit 0. After consolidation, each file's [0] index, lost-as-const and
readonly string[] annotation mutations fails its named schema test at 101;
the cutover accepted-variant inventory independently fails the index control.
All 25 affected tests pass after restoration. Counts remain 316/108/2; this is
an oracle repair, not new runtime behavior or a new closed manifest claim.

## Public ticker mapping

313 pending / 111 ported / 2 divergent. The serialize-only ticker contract and
two explicit mappers compare complete ordered results to live TS over 1,560
ticker inputs and 1,585 wrapped payloads. Inputs include committed public
evidence, every missing/null/wrong field, optional-field combinations, positional
arrays, and pairwise invalid fields. No live venue was contacted. Percentage
null is omitted; null funding/volume remains invalid. Base amounts and open
interest retain the source base-unit values.

Independent Rust/TS null-percentage and error-precedence mutations each fail
`public_ticker_declaration_and_complete_mapping` at 101. Dropping serialized bid
also fails that test. Restored 85 tests, fmt, plain strict Clippy, wasm and
parity-check pass (6,901 rows / 1,312 public items). Shared tuple-oracle repair
received MERGE after seven independent negative controls.

## Public instrument mapping

309 pending / 115 ported / 2 divergent. The serialize-only instrument record,
anonymous singleton literals and tick steps have a bidirectional source-schema
comparison. The live TS oracle compares complete ordered results/errors across
1,561 instrument cases, 4,763 discovery arrays and 123 tick-step inputs. Discovery
validates all records before filtering, all supported instruments before duplicate
checks, and preserves input order. Optional null remains distinct from absence;
an own option_type key is rejected even when null. Inputs are finite JSON.

Independent Rust and TS substitutions that accepted null option_type each failed
`public_instrument_declaration_and_complete_mapping` at exit 101. Both restored.
All 87 venue tests, fmt, plain strict Clippy, wasm check and parity-check pass
(6,901 rows / 1,321 public items), with Cargo limited to two jobs. Prior ticker
review returned MERGE after independent wrong-open-interest-field controls.

## REST book and candle mapping

304 pending / 120 ported / 2 divergent. Five source functions now use the shared
OrderBook/Candle types and shared sort_order_book/calc_order_book_total. The live
oracle executes the actual TS orderbook utilities: 912 level/context cases, 633
snapshots, 1,098 candle/context cases, 231 chart payloads and 417 parallel-array
chart responses. Corpus includes equal-price stability, duplicate/unsorted ticks,
empty arrays, mismatched lengths, no_data with invalid fields, and validation
precedence. Shared Candle.lastSide remains absent as in the source mapper.

Rust and TS accumulate-before-sort substitutions each fail
`public_rest_book_levels_sort_then_accumulate` at 101; separate milliseconds-not-
seconds substitutions fail `public_chart_mapping_errors_units_and_parallel_arrays`
at 101. Restored all 89 tests passed. Clippy initially found three needless borrows;
after removal, strict Clippy and all ten public tests pass, as do wasm and parity
(6,901 rows / 1,326 public items). Instrument review returned MERGE after its own
Rust/TS reversed-step-order controls both failed at 101.

The first book/candle review held delivery on a computed-number oracle gap:
finite input amounts can overflow the accumulated total, while both JSON
serializers collapse Infinity and NaN to null. Reviewer independently substituted
NaN for overflowing totals in Rust and TS; each wrongly passed all ten public
tests. Source implementation was correct, but that numeric identity claim was
unproven. The oracle now preserves non-finite classification and sign before
serialization (recursive TS projection and raw Rust total projection). Repeated
independent Rust/TS substitutions each fail the named book test at 101. No new
runtime divergence is claimed; counts remain 304/120/2.

## Public numeric-depth and request contracts

302 pending / 122 ported / 2 divergent. The eight numeric book depths are a
serialize-only enum with one set of numeric discriminants and an outbound u32
conversion. The shared strict tuple oracle now supports numeric literals while
retaining the same as-const/index/annotation checks used by its string consumers.
The public request generic mirrors the existing session request adaptation and
compares required fields, generic names, defaults and constraints against TS.

Independent Rust/TS changed-depth controls fail both named literal tests at 101;
TS index narrowing fails the declaration test. Removed Rust method constraint
and narrowed TS params constraint each fail the named generic test at 101.
Restored 90 tests, fmt, strict Clippy, wasm and parity pass (6,901 rows / 1,328
public items). Book overflow repair received MERGE after the reviewer repeated
both runtimes' NaN and negative-infinity substitutions, all four at 101.

The initial request-type review caught a serialization blind spot: an injected
skip_serializing on method passed the AST and decoder checks. Complete concrete
request serialization now compares to the actual live TS get-instruments/ticker
builders, without claiming those builders are ported. Skipping method or params
in Rust, and removing method or a required params field in TS, each fails
`public_request_complete_serialization` at 101. Restored twelve public tests and
strict Clippy pass; counts unchanged at 302/122/2.

## Computed-number oracle sweep

Counts remain 302 pending / 122 ported / 2 divergent. REST order arithmetic is
the other implemented mapper that can compute non-finite outputs from finite
inputs. Its corpus now has 422 cases with 91 NaN, 32 positive-infinity and 40
negative-zero output occurrences. Those values match live TS; finite-size
safe-math overflow/NaN behavior is preserved and marked on the existing row.
The shared test projection retains numeric identity before JSON for both order
outputs and all three numeric book-level fields. Other implemented numeric
mappers validate finite values without overflow-producing arithmetic; candle
milliseconds division cannot overflow.

Independent Rust/TS NaN-to-Infinity and negative-zero-to-positive-zero controls
all fail `rest_order_configuration_sizing_and_raw_info` at 101. Restored fourteen
affected tests, fmt and strict all-target Clippy pass. The public-request review
returned MERGE after independently repeating all four required-field omissions.

Follow-up review found that book fixture canonicalization itself erased input
negative zero, so a sign-collapsing amount mutation still passed. Two raw JSON
book fixtures now preserve -0 through independent JS/Rust parsing. Rust and TS
mutations collapsing only zero amounts to positive zero each fail the book test
at 101; restored fourteen affected tests and strict Clippy pass. The expanded
order counts above are synthetic boundary-corpus output occurrences, not live
venue captures. Numeric projection is an oracle boundary, not a production
serialization change. Counts remain 302/122/2.

## Delta-level parser

301 pending / 123 ported / 2 divergent. parseDeltaLevels and its helper-local
action/frame/record declarations now have source-schema, serializer and decoder
classification coverage. The live oracle compares 4,170 result/error cases,
including exact array arity, snapshot-only-new actions, validation precedence,
Unicode contexts and raw JSON signed-zero delete amounts. The sequenced-book
class remains pending on the shared implementation; no local state machine.

Independent Rust/TS controls accepting zero new/change amounts or allowing change
actions in snapshots each fail `public_delta_level_actions_and_error_precedence`
at 101. Restored 92 tests, strict Clippy, wasm and parity pass (6,901 rows / 1,332
public items). The raw book negative-zero repair received MERGE after independent
sign-collapse controls, and is pushed with the numeric output sweep.

Delta review found that assigning numeric tags could recreate a field omitted by
Serde, so a skipped delta price passed. All order/book/delta projections now
require the serialized field to exist and equal ordinary serialization of the
raw f64 before substituting its identity tag. An actual delta skip_serializing
mutation, a delta rounding serializer, and omitted serialized book/order fields
each fail their named test at 101. Restored sixteen affected tests and strict
Clippy pass. Counts remain 301/123/2; no runtime behavior changed for the repair.

## Integration and cutover validator

Merged a6f317fbc9 into a6d9c5b18c, retaining the implemented Coinbase dependencies.
Named CI steps: own parent 56, incoming parent 55, merged 56; no names lost
against either parent. Layer audit passes (37 classified crates), CI wiring
passes 105 tests, and the scoped venue passes all 92 tests after integration.

validateEvidence compares 2513 live-source cases, including undefined,
wrong root shapes, all evidence combinations, missing/status precedence, and raw
JSON with deliberately unsorted index keys. Raw input avoids JSON.stringify
pre-sorting keys before Rust receives them. Empty first unknown string keys
suppress subsequent unknown rejection exactly as TypeScript does (ts-bug).
Independent Rust insertion-order and TS reverse-order mutations, plus separately
removing the empty-key truthiness check in Rust and TypeScript, each fail
cutover_validation_own_key_order_and_empty_key_truthiness at exit 101. Restored
source passes. Symbol keys are outside this declared finite JSON boundary.

The isolated adversarial reviewer approved 51da0228c7 and d73709e5f7, repeated
Rust insertion-order and TypeScript lexicographic-order negative controls at
101, and passed all 93 restored venue tests plus layer and CI wiring audits.

## Public REST builders

Get-instruments, ticker and order-book builders compare 24,553 live-source
cases: every declared depth, rejected scalars/containers, saved-symbol grammar
and simultaneous invalid depth/symbol inputs. Output compares actual complete
serialized builders (method and params), not an AST approximation.
Independent Rust and TypeScript expired=false-to-true mutations and separate
Rust depth membership / TypeScript invalid-depth acceptance mutations fail
public_rest_request_builders_and_depth_precedence at exit 101. Restored public
suite passes 14 tests. No live endpoint is contacted.

Reviewer approved 0d2888d371 and independently reversed depth/symbol validation
in each runtime; both fail the named request test at 101. Restored14 pass.

## Instrument market parity result

The serialize-only result has a blocked-only status and borrows its gateway,
preserving source identity rather than silently cloning. The maintained schema
checks the complete nested shape; 104 valid normalized instrument cases compare
actual source outputs and assert reference/pointer identity. Independent Rust
and TypeScript reversed-blocker controls and independent gateway-clone controls
all fail public_market_parity_retains_gateway_and_ordered_blockers at exit 101.

The market-parity reviewer approved 46cc708512, independently injected Rust
gateway omission and TypeScript status omission, and measured each named test
failing at101. The identity/order mutations above were the author's controls,
not independently repeated by that reviewer. Restored16 affected tests passed;
all-target clippy, scoped wasm and parity gates also passed.

## Chart request and resolution

29 resolution and1189 chart-request cases evaluate live TypeScript, including
all own timeframe entries, Object.prototype names, simultaneous invalid inputs,
equal/reversed timestamps, and raw signed-zero timestamps. Numeric projection
retains presence and ordinary serializer checks before special-number tagging.
The deliberate CORRECTED divergence rejects inherited resolutions: TypeScript
returns native functions for constructor/toString (omitted from serialized
request params) and an object for __proto__. Rust chooses own string resolutions;
a broader value representation could model the original, so this is not FORCED.
Both sides are asserted rather than excluding prototype cases from the corpus.
Independent Rust/TS equal-timestamp rejection and wrong-method mutations each
fail public_chart_request_order_resolution_and_prototype_correction at101.
Allowing constructor in Rust and changing TS to own-key-only lookup also each
fail that test at101, proving both declared halves of the divergence are pinned.

The chart reviewer approved 9a5e339e08 and independently repeated symbol-before-
timestamp mutations in Rust and TS plus Rust negative-zero erasure; all named
failures were101, and the restored public suite passed16 tests.

## Sequenced book adapter

2405 stateful scenarios /15222 steps evaluate the actual Coinbase TypeScript
class with the actual shared TS sequencer and updateOrderBook utility. Rust
calls safe_cex_store::sequenced_delta_order_book rather than copying it. The
corpus covers malformed frames, validation precedence, missing snapshot,
duplicate/stale/gap recovery, action conflicts, both sides, reconnect, external
book-handle mutation, overflow totals and persistent identity on every step.
Author controls independently disable waiting, omit total calculation, and
retain hasSnapshot on reconnect in each runtime; all six named controls exit101.
The first Rust total control accidentally edited the REST mapper's identical
call and passed the adapter-only test; it was corrected to select the class
body uniquely, then failed101. This was a mutation-target error, not evidence
that removing the adapter call was harmless. All source mutations restored.
A replayed new-level delta (and stale conflicting new action) recovers invalid-
level because Coinbase validates action membership before the shared sequencer
can ignore duplicate/stale IDs. This TS defect is preserved and marked.

The sequenced-book reviewer approved cca16c6fa3, independently retained deleted
prices in the action set in both Rust and TS and measured named101 failures.
Restored18 affected tests passed; own full97 venue tests, clippy, wasm and parity
also passed before review.

## Cutover readiness and fixed selector

27,643 live-source readiness cases cross evidence validation with accepted and
rejected product values, retaining validation precedence and optional-plan
omission. The production selector is exercised directly; source arity and the
Rust fn-pointer signature both pin zero caller arguments. Fixed evidence,
activation constants, complete closed output and blocker ordering are compared.
Independent Rust/TS controls for unconditional plan inclusion and omitted raw
validation fail the readiness test at101. Independent Rust/TS credentialed HTTP
auth evidence changes and omitted runtime-disabled blocker fail the production
selector test at101. All eight mutations restore cleanly. Existing strict DTOs
are constructed fallibly; no panic/expect is introduced on a data path.

The readiness reviewer approved e0e375d551 and independently removed the runtime
activation blocker in Rust and TypeScript; each named selector test failed at101.
Restored full99 venue tests,115 CI wiring tests and37-crate layer audit passed.
The reviewed integration merge f6972d001f retains all named steps from both
parents: e0e375d551 had56,35e1b03e7f had62, and the merge has63, with no losses
against either parent. Coinbase dependencies remain present and the crate
compiles after the merge. Measured manifest:287 pending /135 ported /4 divergent.


## Public channel template contract

The serialize-only channel payload enum retains the four template branches and
closed suffix domains without accepting arbitrary whole-channel strings. The
maintained TS AST comparison measures branch heads, every interpolation domain,
and literal tails against Rust payload field domains.158 serialization cases
cover every suffix with empty/dotted/Unicode/BOM/NEL/NUL instrument names. The
serializer consumes existing enum serialization rather than copying spellings.
Independent Rust and TS ticker-prefix mutations each fail
public_channel_template_domain_and_serialization at101. Restored19 affected
public-data and decoder-boundary tests pass; new types expose no input decoder.


The channel-template reviewer approved17ca488cce and independently altered a
Rust suffix, widened a TS suffix domain, and removed a TS branch. Each named
template test failed at101; restored19 affected tests passed. No decoder bypass
was found. The reviewed commit is pushed.

## Public channel builders

59,377 live-source cases cover four channel builders, interval defaults and
rejections, currency membership, symbol/timeframe validation order and complete
channel strings.48 successful inherited-property TS chart channels are pinned
as source behavior and separately asserted as Rust ProtocolError, carrying the
resolver's deliberate CORRECTED policy. Both Rust and TS default-interval changes
and symbol-before-interval changes independently fail
public_channel_builders_validation_precedence_and_prototype_correction at101.
All four mutations restored;20 affected public/decoder tests pass.


The channel-builder reviewer approved586e8361c0 and independently widened
currency acceptance to Any in Rust and TS, then moved TS resolution before
symbol validation. All three named controls failed101; restored20 tests passed.
The reviewed commit is pushed.

## Public subscription state and acknowledgement

632 live-source scenarios /7426 steps cover empty/duplicate rejection, requests
before state changes, absent removal, repeated subscription, delete/re-add order,
reconnect omission and independent input/output arrays.19,592 acknowledgement
cases retain shape-before-length-before-membership behavior, including reordered
responses and the marked repeated-request defect ([a,a] accepts [a,b]). The
method constraint is separately compared as a closed serialize-only union.
Independent Rust/TS state-before-validation controls each fail the named state
test at101. Independent Rust/TS first-element-only membership controls each fail
the named acknowledgement test at101. All mutations restored;103 scoped venue
tests pass, including ignored gates.


The subscription reviewer approved3e80fee3ac, independently omitted the TS array
copy, inserted Rust subscriptions at the front, and reordered repeated TS
subscriptions. Each named state test failed101; restored22 affected tests passed.
Clippy and scoped Wasm passed before review; the reviewed commit is pushed.

## Private protocol unions

Ten closed unions /51 literal values compare live TS declarations and executed
constant tuples against Rust variant domains and serialization. The source
inventory includes every indexed or pure literal-string alias; tuple inference
uses the common strict-as-const helper. All enums are serialize-only and included
in the explicit decoder-boundary classification. Independent Rust and TS
cross_pm spelling changes fail private_protocol_union_serialization at101.


The private-union reviewer approved d84fa4d328, independently removed TS as const,
added JPY to the TS fee union, and skipped Rust EURR serialization. All three
controls failed named tests at101; restored3 affected tests passed. The reviewed
commit is pushed.

## Private account, position and history records

Five serialize-only records retain declared field/generic constraints and two
anonymous closed unions.306 optional-field combinations cover every mask for
account summary, position, history options and page parameters; absent fields
omit while present false and numeric values remain. Request output field names
are derived from the live TS declaration too. Independent Rust omission removal
and TS optional-to-required mutations fail private_record_complete_serialization
at101; both restored. No implicit record decoder accepts arrays or explicit null.


The account/position/history reviewer approved e01bffa7e0 and independently
removed a TS optional field, skipped a required Rust field, and narrowed the TS
generic record constraint. All named controls failed101; restored5 tests passed.
The reviewed commit is pushed.

## Remaining private record and result contracts

The declaration oracle now covers every private exported type.835 record cases
include both order-price branches and all optional masks, plus both generic
ready/blocked result branches. The Rust schema walker distinguishes untagged
newtype unions and internally tagged object unions, checking their fields and
generic parameters. No input deserializer is exposed by these output contracts.
Independent Rust tagged-price and TS changed-price-sentinel controls each fail
private_record_complete_serialization at101. Independent Rust changed tag-key
and TS changed ready-status controls each fail
private_parity_result_tag_and_branch_fields at101. All mutations restored.


The remaining-type reviewer approved4d12983718 and independently removed the
TS optional trade profit/loss field, skipped Rust blocked-result gateway, and
skipped Rust optional trade profit/loss serialization. All named controls
failed101; restored6 affected tests passed. Full108 venue tests and clippy passed
before review. The reviewed commit is pushed.

## Private guards

12,546 cases execute the actual private TS guards, covering finite JSON/missing
inputs, strict numeric/string/boolean/enum checks, exact errors, Unicode contexts,
collection identity, count1..1000 and signed zero. Raw input JSON and tagged
numeric results preserve -0 before transport serialization can erase it. Shared
Coinbase guard delegation is checked against the separate private source body.
Independent Rust/TS null-as-missing mutations and excluded-count1000 mutations
each fail private_guard_results_identity_and_signed_zero at101; all restored.


The private-guard reviewer approved6a75c5147e, independently erased signed zero
in both Rust and TS and changed the Rust negative-number diagnostic. Each named
guard test failed101; restored7 affected tests passed. The reviewed commit is
pushed.

## Private REST builders

35,545 cases evaluate actual source builders plus the actual public symbol
converter. History options exercise all3125 missing/null/false/true/zero masks,
individual field failures, simultaneous failures, unknown keys and raw -0 offset.
Trade requests distinguish missing count default100 from rejected null; history
count defaults on both null and missing. Timestamp bounds and symbol/option
validation order, complete method/params and field ordering are compared.
Independent Rust/TS null-default removal and count-before-timestamp mutations
fail private_rest_builders_defaults_omission_and_validation_order at101; restored.
Declared options use an object-map boundary, rather than claiming primitive or
null option containers are accepted. No request is sent.

The REST-builder reviewer approved e9015e10fa after independently removing
extended in Rust and TypeScript and historical:false in Rust. Each control
failed the named builder test at101; restored8 affected tests passed. Pushed.

## Account summary and balance assessment

The actual TS decoder and assessor run on malformed records, every field's
invalid inputs, every pair of null fields to pin validation precedence, all256
optional masks across4 margin models, and raw JSON negative zeros. Outputs
compare complete field order and numeric tags before JSON can erase signed zero.
The balance assessment always blocks and retains the original account identity.
Rust uses an empty Serialize enum for TypeScript never; no ready value exists.
Independent Rust and TS balance/equity swaps and blocker-removal controls each
failed private_account_decoder_and_balance_blocker at101. Restored9 affected
tests, scoped all-target clippy, and parity invention/status gates pass.

Reviewer independently repeated Rust/TS total_pl-to-session_upl mutations at101,
but found a return-type oracle gap: TS never-to-unknown passed all8 tests. The
new live source return-type comparison checks both generic arguments against
the Rust assessor, including borrowed gateway and the empty never enum. Rust
Never-to-Value and TS never-to-unknown now fail the named return-contract test
at101; restored10 affected tests pass. Account corpus count is2365.

The account return-contract re-review approved3b61eac1b5: independent TS
never-to-unknown, Rust Never-to-Value and TS gateway-type widening each failed
the named return-contract test101. Restored10 affected tests passed.

## Private position decoder and collection helpers

Live source coverage:1072 single positions,1081 position lists,258 instrument
helper inputs and3695 unique-ID callback scenarios. Includes distinct wire
values, all field-null pairs, signed sizes/zero direction, finite input whose
notional overflows, optional leverage, malformed later records after duplicates,
ordered callbacks and callback errors. Numeric results are tagged before JSON
and compared against actual Rust fields; position assessment retains identity.
The position assessor also shares the live generic return-contract check.
Independent Rust and TS absolute-value removal and duplicate-acceptance controls
each fail their named position/helper tests at101. Restored13 affected tests,
scoped all-target clippy and parity invention/status gates pass.

Position review independently caught Rust/TS overflow-sign mutations at101,
but a copied callback argument passed the value-only helper corpus in both
runtimes. The corpus now includes object elements and asserts original element
identity in TS and pointer identity in Rust on each call (7390 callback cases).
Independent author Rust id(&value.clone()) and TS shallow-copy controls now
fail private_instrument_and_unique_helpers at101. Restored13 affected tests pass.

The position re-review approved3bfa7d179e after independently repeating both
callback-clone bypasses and observing named101 failures; restored13tests. Pushed.

## Private order decoding and history

The maintained live source oracle covers2189 single orders,17584 list/history
cases,195 order-state cases and40 price cases. It checks all enum domains, all
optional masks, pairwise invalid-field precedence, raw signed zeros, omitted
fields, duplicate IDs, expected-ID priority and expected-symbol priority.
Independent Rust/TS filled-amount-to-amount substitutions and expected-ID
validation reorderings fail the named decoder/list tests at101. Restored15
affected tests, scoped clippy and parity invention/status gates pass.

Order review approved7bd7c85136 after independent Rust/TS post_only-before-trigger
mutations and TS incremental-duplicate checking failed named tests101. Restored
15tests passed; pushed.

## Private trade decoder and blocked normalization

2058 live trade cases and4410 page cases cover required-null matching_id, optional
liquidity/label/contracts/PnL, all enum values, pairwise invalid-field priority,
safe sequence and timestamp bounds, signed-zero fee/PnL and duplicates after
complete decoding. Normalization retains the exact gateway reference and its
trade-amount-unit-conflict blocker. Its live HistoricalTrade return argument is
also checked, without silently substituting never just because it always blocks.
Independent Rust/TS missing-matching-ID acceptance and blocker-removal controls
each fail private_trade_decoder_and_blocked_normalization at101. Restored17
affected tests, scoped clippy and parity invention/status gates pass.

The order normalizer remains pending: actual TS source retains gateway identity
under ready.value.info, including later gateway mutations and NaN. The shared
Order.info JSON-value representation cannot carry that alias or non-finite
number. Routed centrally; no venue-local Order replica or silent clone introduced.

Trade review approvedf9d3877eea after independent Rust/TS order_type-before-liquidity
controls and Rust missing-has_more diagnostic substitution failed named tests101.
Restored17tests passed; pushed.

## Private order mappings

All26 declared-domain mapper combinations execute actual TS bodies against actual
shared TS enums, including both postOnly flags for every time-in-force. Rust/TS
post-only precedence and unsupported-type fallback mutations each fail
private_order_mapping_domains_and_results at101. Restored18 affected tests,
scoped clippy and parity invention/status gates pass.
The shared Order.info identity/NaN seam now has a permanent source-only witness
in the maintained private oracle; this is not a claim that normalization is ported.

Private mapper review approved781d306b47 after independent Rust/TS LastPrice
to MarkPrice controls failed named101; restored18tests passed, pushed.

## Write-contract literal unions

Five serialize-only unions pin all20 values to live TS aliases and executed
protocol tuples. Shared strict tuple inference requires as const and a number
index; no permissive Deserialize API is added. New write_orders test runs under
the existing mandatory venue-oracle:coinbase all-target invocation and isolated
TypeScript installation. Independent Rust/TS first_hit spelling changes fail
write_protocol_union_domains_and_values at101, as does TS removal of as const.
Adding derived Deserialize fails every_public_wire_decoder_entrypoint_is_classified
at101. Restored2 affected tests, scoped clippy and parity gates pass.

Write-union review approved21f2bf9659 after independent TS index-[0] narrowing,
added literal alias and Rust skipped CompleteFill controls failed the named test
at101. Restored2tests passed; pushed. Coherent private/write-union checkpoint:
all121 Coinbase tests and scoped wasm check passed with jobs2.

## Write amount, gate and timestamp records

Six source type rows retain source/unit singleton fields, untagged base/contract
amount alternatives, status-tagged ready/blocked generic fields, unresolved
timestamp unit, and all8 gateway-TIF/flag combinations. Complete branch witnesses
complement live TS schemas; cross-module Rust schema resolution rejects duplicate
type declarations instead of silently overwriting one. No decoder is introduced.
Independent Rust/TS source-field omission plus Rust added amount tag and TS
removed contract branch each fail write_intent_records_and_gate_branches at101.
Restored all122 scoped Coinbase tests, clippy and parity gates pass.

The record reviewer independently caught Rust blocked-intent omission and TS
Intent-to-T payload mutations at101, but found an imported-type oracle gap:
changing the gateway TIF import to OrderState remained green. The oracle now
resolves the actual ImportSpecifier and checks its module before deriving the
private alias schema. Changing the imported symbol or module each now fails
write_intent_records_and_gate_branches at101; restored3 affected tests pass.

The import-resolution reviewer approved b9c54c4f88 after independently repeating
the imported-alias and wrong-module controls at named101; restored3 tests passed.

## Edit and close-position contracts

Three source rows retain all optional fields and closed limit/market values.
The maintained source schema and serialized witnesses cover512 edit-intent cases
(two amount branches across256 masks),64 edit-param masks and4 close-position
cases. Present false flags remain present; absent fields omit their keys.
Independent Rust skipped postOnly and TS removed postOnly mutations each fail
write_intent_records_and_gate_branches at101. No decoder is introduced.

Edit/close review approved8df68eb902: independent TS triggerOffset removal, Rust
skipped serialization and TS close-type widening each failed named tests at101.
Restored3tests passed; pushed.

## OTOCO contract

Fixed two-leg tuple and shared safe_cex_types::True retain source shape; reachable
leg and anonymous order type map to the enclosing row.64 optional masks plus288
combinations of direction/core TIF/trigger/order type/fill condition compare actual
serialization, with distinct first/second labels pinning tuple order. Imported
types resolve actual source import bindings. Rust reduceOnly rename, TS true-to-
boolean widening and TS tuple-to-array widening each fail
write_intent_records_and_gate_branches at101, independently and restored.
Restored all123 Coinbase tests, scoped clippy and parity gates passed.

OTOCO reviewer approved4d57b156b6 after independent second-tuple-member, imported
trigger and omitted Rust nested-size mutations failed named tests at101. Restored
4tests passed.

## Order identity contract

All4 optional masks across empty/singleton/ordered duplicate child arrays compare
actual serialization against live source shape. Canonical ID and noncanonical
label carry distinct values. Independent Rust label rename and TS child-array to
string mutations fail write_intent_records_and_gate_branches at101, then restore.

Identity reviewer approved99a597c814: independent Rust omission, TS optional-field
removal and array-to-tuple narrowing failed named101; restored4tests passed.
Canonical declaration0716cd9257 merged: named CI steps63/62 on parents,63 merged,
no loss. Native and wasm Coinbase compile and all78 layer tests passed (the first
layer invocation lacked cargo on PATH; rerun with the required PATH passed).

## Write result envelope

The envelope consumes existing private order/trade records; its live schemas
resolve actual import bindings and each nested alias in its owning source file.
Actual serialized witnesses cover empty/single/ordered two-trade outputs and both
price alternatives. Independent Rust skipped trades and TS imported trade alias
changed to order each fail write_intent_records_and_gate_branches at101.

Write-result reviewer approvede7c9cdc66e after independent Rust identity omission,
TS order-import substitution and nested optional-field removal failed named101.
Restored4tests passed; pushed.

## Placement intent and parameter records

Three source rows map the placement intent, its exact Omit-side projection, and
wire params with reachable OTOCO leg params.4096cases each cover all masks on the
three public records (both amount branches for the intents), plus64 leg-param
masks. Fixed two-leg tuples, indexed leg-type lookup and the shared True field
remain closed. Independent Rust skipped reject_post_only, TS changed Omit key,
and TS wire tuple narrowing each fail write_intent_records_and_gate_branches at
101 before restoration. No Deserialize or builder behavior is introduced.

Placement reviewer approved391d0bd2e7: independently substituted indexed leg
field, expanded Omit keys and omitted Rust otoco_config; each named101, restored
4tests passed.

## Instantiated write request aliases

Concrete request records and closed method enums preserve both TS aliases without
adding generic-bound-only object conversions. The oracle resolves the imported
PrivateRequest declaration, constraints, required fields and both substituted
arguments; template-literal methods expand from the actual side domain. Rust
skipped method, TS method prefix change and TS imported generic params becoming
optional each fail write_intent_records_and_gate_branches at101, separately.

Request reviewer approvedcc767b58ed: independently misbound params to Method,
narrowed the Record constraint and omitted Rust params; named101, restored4tests
passed. Pushed.

## Write numeric validation and amount builders

Actual source bodies drive all float inputs, with boundary values and deterministic
random binary64 bits. Non-number positive-guard inputs reject without coercion.
Timestamp validation retains negative safe integers and -0; fractional positive
contract totals remain accepted. Special-number projection checks the real
serialized field before tagging, so it cannot recreate a skipped field. Numeric
JSON spellings compare as binary64, while tags preserve nonfinite/-0 identity.
Independent Rust AND TS mutations for zero acceptance, widened safe-integer bound
and timestamp negative-zero erasure each fail
write_numeric_builders_live_values_errors_and_float_bits at101 and restore.
Measured19,602 live-source numeric cases; restored5 affected tests, scoped clippy
and parity gates passed.

Numeric review approvedc7180be511: independent Rust/TS positive-subnormal rejection
and Rust negative-timestamp rejection each named101; restored5tests passed.

## Write collection and string validators

Borrowed object/array results retain identity. String-array decoding validates all
elements before duplicate checks, including a duplicate preceding an invalid
third element. Label length counts UTF-16 code units, preserving astral, combining,
BOM/NEL and boundary behavior. A source-only64/65 lone-surrogate witness and Rust
JSON rejection pin the shared representation boundary; validateLabel is explicitly
divergent with the canonical CHOSEN shared-repr:utf8-string citation, without
claiming that the length rule changed. Other guards retain their stated existing
JSON/Unicode-scalar boundary. Independent Rust AND TS code-point label counting,
empty-string acceptance and early duplicate detection each fail
write_collection_guards_live_errors_identity_and_validation_order at101.
Measured5,182 guard cases plus2 surrogate witnesses. Restored all126 Coinbase
tests, scoped clippy and parity gates passed.


Guard reviewer approved96451940f9: independently reproduced Rust/TS null-as-missing
and Rust rejection of exactly64 UTF-16 code units; named101, restored3tests passed.

## Write-result decoding and assessment

Full live private and write modules execute with actual protocol and core enum
bindings; selected public symbol helpers retain their source bodies and imports.
Measured1,739 decoder cases,421 standalone identity cases and22 assessment cases.
The decoders retain validation precedence, decoded canonical identity and child-ID
order. Assessor branches borrow the original envelope, with trades before fills,
and tagged NaN/infinities/negative-zero payloads. Nested serializer checks precede
numeric projection so a missing field cannot be reconstructed by the oracle.

The result type declaration orders identity first, but both actual factories emit
order/trades/identity. Structural schema comparison ignores only this root record's
field order; executed factory comparison pins runtime order instead. Independent
Rust AND TS canonical-ID corruption, zero-filled blocking and identity-first
serialization each fail the named write_result_decoders_live_validation_identity_and_serialization_order
or write_result_assessment_live_blocker_precedence_float_bits_and_borrow test at101.
All controls restored; mutation logs remain under the ignored worktree .aimux.
Restored23 affected tests, scoped all-target clippy and parity-check passed
(6,906 manifest rows;1,533 public Rust items).

Write-result reviewer held eb2dc13cd6: widening the TS ready return argument to
unknown passed runtime-only tests. The added live return-contract test compares
both actual generic arguments against the Rust signature and requires immutable
borrowed payloads. Reviewer independently moved order decoding ahead of trades
shape validation in each runtime, obtaining named101 failures; restored6tests
passed before the contract repair.
Independent source ready-unknown and blocked-never mutations now fail
write_result_assessment_live_return_contract at101; restored3 result tests pass.

Write-result repair reviewer approved2032492b86: independently widened each TS
return argument, named101; restored7tests passed, Cargo idle.

## Time-in-force mapping and cancel request

1,122 full-source cases cover every current shared TIF spelling, tagged-object
and unknown-shape negatives, cancel ID lengths0..129, whitespace/BOM/NEL,
supplementary scalars and embedded NUL. Shared OrderTimeInForce deserialization
supplies the canonical spelling table; errors remain venue-specific. Cancel
preserves strings verbatim and emits only method/params/order_id.
Independent Rust AND TS lost-post-only and wrong-cancel-method controls fail
write_basic_builders_live_values_rejections_and_payload_order at101. Restored5
affected tests, scoped clippy and parity gates passed (6,906rows;1,535items).

TIF/cancel reviewer approved92acfe9eee: independent Rust FOK-to-IOC and TS ID-trim
controls failed the named builder test101. Restored5tests passed; pushed.

## Edit request construction

2,886 source cases cover all256 optional-field masks; the Cartesian product of
missing, negative, zero, subnormal, maximum-finite and nonfinite prices/trigger
fields; and conflicting ID/size/deadline/post-only conditions. Blocked results
borrow their input, checked by identity in both runtimes; float projection checks
the actual serialized payload first. A live generic return comparison also checks
the Rust borrowed blocked-intent type instead of relying only on output values.

The edit factory inserts price before post_only and trigger_price before
trigger_offset, unlike its structural declaration. Actual builder output is
compared in insertion order; type-only checks ignore this record's field order.
The first restored type run caught the nested EditRequest schema carrying the
same declaration order; the exception now applies to its params schema too,
without discarding field names, optionality or types. Restored5tests passed.
Independent Rust AND TS wrong-size-blocker and false-flag-as-no-change mutations
fail edit_builder_live_blocker_identity_validation_order_and_payload at101; a TS
ready-return widening also fails edit_builder_live_return_contract at101.
Restored5 affected tests, scoped all-target clippy and parity-check passed
(6,906 rows;1,536 public Rust items).

Edit reviewer approved48e167ef97: independent Rust trigger-price-from-offset and
TS size-before-ID controls named101. Restored6tests passed; pushed.

## Close-position construction

242 full-source cases cross both declared order types, valid/invalid saved symbols,
missing price and binary64 boundaries including NaN/infinities/-0. Shape guards
precede symbol resolution, then positive finite price validation. Actual request
method, params key order and omission are compared against executed source.
Independent Rust AND TS symbol-before-shape mutations each fail
close_builder_live_shape_symbol_price_precedence_and_payload at101 and restore.
The restored full scoped crate test run, including ignored cases, passed: 16 test
binaries and all tests reported `ok` (command used `--jobs 2 --test-threads=2`).
Close reviewer approved3156b9fd8c: doubled-price and price-before-type payload
mutations independently failed the named test101. Restored scoped suite5tests
passed; the full Coinbase include-ignored suite also passed (16 binaries).

## Direct typed numeric record ingress

The numeric-field audit covers13 public f64 fields in MarketResponse,
IntxInstrumentsResponseItem and PortfolioBreakdownResponseBreakdownSpotPositionsItem.
Independent TypeScript JSON.parse witnesses establish negative-zero bits and raw
exponent overflow to Infinity. For each field, raw typed `from_str` preserves
integer-token -0 by f64 bits, and `from_value` from an in-memory floating
`Value(-0.0)` also preserves it. However, parsing the JSON token -0 into
`serde_json::Value` first erases the integer sign; subsequent typed `from_value`
returns +0. The source lexeme cannot be recovered after that Value construction.
All13 parsed-Value cases are pinned as this finding; the three enclosing manifest
rows are `divergent`. No Rust production callers of these DTOs were found, so the
evidence describes their public decoder paths and does not claim a production
entry path. Raw exponent overflow is a separate shape and remains independently
measured on raw typed and owned-Value decode. Balance, Market, Order, Position,
and Ticker shared graphs remain ineligible under MANIFEST-SPEC section17.

The raw-token guard is independently controlled: changing -0 to +0 fails
`public_numeric_record_decoders_distinguish_negative_zero_ingress_paths` at101. Changing
the TypeScript oracle ingress from JSON.parse('-0') to JSON.parse('0') also fails
the same named test at101. Both mutations were restored. No production decoder
behavior changed.

## Coinbase Starbase read-only state

Eight rows are ported: `CoinbaseStarbaseReadOnlyState` and its seven update
methods. The maintained TS oracle exercises 14 sequential valid/invalid calls
across all methods. Each case compares the returned value or protocol error and
the entire state afterward, proving a failed decoder does not overwrite the
last valid snapshot. `private_read_only_state_replaces_only_after_complete_validation`
and `private_record_declared_shapes` pass. Independent TS and Rust assignment
mutations are checked against the named state test. At this read-only-state
checkpoint the counts were 163 pending, 254 ported, 9 divergent. The Store
notification audit was repeated as a reachability check, following Bybit's
`STORE_NOTIFICATION_AUDIT.md` method. At that checkpoint, 138 ported callable
manifest rows resolved to source declarations; the
source call graph reaches no Store notification from any of them. Thirteen
direct Store notification calls in Coinbase TypeScript are owned by ten pending
exchange/socket methods: `CoinbaseExchange.start`, `_startPublicDataOnly`,
`tick` (four sites), `markPrivateUserStreamMessageReceived`,
`markPrivateUserStreamDegraded`, `#reconcileOpenOrdersFromRest`, `cancelOrders`,
`placeOrder`, `CoinbasePrivateWebsocket.handleUserEventOrders`, and
`#applyOrderSnapshotBatch`. No Coinbase WebSocket lifecycle entry row is
ported. The ported sequenced book only calls the shared sequenced-book
operation, which mutates its `OrderBook` handle and has no Store notification
path. Thus the measured reachable count is zero; this is not a direct-call-only
scan. The latest census resolves 142/142 ported callable rows and still finds
zero reachable chains, 13 direct sites and 10 pending writer rows. Current
manifest counts are now 258 ported, 155 pending, 13 divergent. `scripts/check-store-notification.mjs` repeats the manifest/source census,
call reachability, and pending-writer classification, and is run by the
maintained `venue-oracle:coinbase` gate. Future handlers crossing this seam must
be marked divergent/FINDING until notification failures are representable.
The current Coinbase Rust crate has no Store mutation call sites. A callee-based
sweep of the Store mutation API found zero Store results converted, stringified,
discarded, or inferred; all 13 TypeScript notification calls remain in pending
rows. This is broader than a `map_err` spelling scan and is documented in
`STORE_NOTIFICATION_AUDIT.md`.
The restored all-target include-ignored suite passed; host and wasm clippy passed
separately with warnings denied. At the then-current head, `parity-check` passed
at 7,334 manifest rows and 3,200 public Rust items. `layers:check` passed 78/78.
The extractor is
runnable in this worktree: its initial check found one stale Coinbase manifest
and zero added/orphaned symbols; the update changed only that manifest.
Integration commit `18261b604c` then landed the same seven canonical blank
separators, and the post-merge check passes with zero manifest updates, zero
additions, and zero orphans. It reports 7,334 symbols across 511 files. Parsed
`exempt.toml` with Python `tomllib`: 327 BLOCKS / 327 DISTINCT.

## Merge-up `228ca8aec8`

Merged reviewed integration `aa632b5578` (91 commits from the Coinbase parent,
29 from integration). Named workflow steps measured 88 on the Coinbase parent,
87 on the integration parent, and 88 in the merge. The per-hunk workflow
resolution retained Coinbase's dedicated live TypeScript oracle step. On the
merged tree, the scoped all-target include-ignored suite and
`venue-oracle:coinbase` pass; host and wasm clippy pass separately with
warnings denied; parity passes at 7,334 rows / 2,955 public Rust items; layer
policy passes 78/78; extractor check reports 7,334 symbols / 511 files / zero
manifest changes, using this worktree's manually provisioned `smol-toml` 1.4.2;
and `exempt.toml` parses as 327 BLOCKS / 327 DISTINCT.

## Starbase order builder slice

Implemented `buildCoinbaseStarbaseBuy`, `buildCoinbaseStarbaseSell`,
`buildCoinbaseStarbasePlaceOrder`, `buildOtoco`, `buildOtocoLeg`,
`validateOtocoLegShape` and `validateOrderShape`. The maintained live-source
oracle drives 35 cases through the actual TypeScript functions, including each
order family, both sides, contract and valid-until blockers, shape failures,
post-only conflicts, OTOCO leg composition and the take-limit migration-guide
exception. Rust request and OTOCO field ordering follows source factory
insertion order; `write_orders` checks the static contract separately.
Independent control mutations changed the Rust `buy` method to `sell`, then
changed the TypeScript buy wrapper to dispatch `sell`; each separately failed
`write_order_builders_live_typescript_corpus` at exit 101 and was restored.
The added-own-property control replaced the TypeScript blocked intent with a
projection omitting `unmodeledRuntimeField`; it failed the named corpus test at
101 and was restored. The tagged-f64 control changed the Rust `-0` input decoder
to produce `+0`; its bit assertion failed at 101 and was restored.

Blocked place-order results retain source object identity, insertion order and
caller-added own properties. Rust's typed intent serializes in its declared
field order and drops unmodeled properties, so the three outer builder rows are
CHOSEN divergences; the corpus pins key order and a caller-added property. A
separate tagged-f64 check compares bits before JSON serialization for a blocked
contracts intent carrying -0 and NaN, because ordinary JSON output erases those
distinctions. The remaining four rows are ported. The scoped
`write_order_builders` and `write_orders` Cargo tests pass. Current measured
Coinbase manifest counts: 156 pending, 258 ported, 12 divergent.

## Starbase private order normalization

Implemented `normalizeCoinbaseStarbaseOrder` with source blocker order, status,
side/type/TIF/trigger mappings, market-price sentinel handling and all optional
Order fields. The 30-case live-source corpus covers all blocker classes,
combined blocker precedence, ready order families, market price, and a
post-normalization mutation proving TS `Order.info` aliases the gateway while
mapped Order fields remain snapshots. One CHOSEN divergent row pins TypeScript
blocked `gateway` and ready `Order.info` retaining caller-added properties and
original own-key order while the typed Rust DTO drops unmodeled fields and the
JSON map sorts keys; it also records the loss of alias identity. The tagged
numeric case bit-checks every direct Order float output and every present
numeric `info` field before serialization: price, amount, filled, remaining,
lastUpdate, takeProfit, stopLoss, averagePrice, creation/update timestamps, gateway
amount/filled/contracts, and triggerPrice. Separate take-market and stop-market
cases pin the `takeProfit` and `stopLoss` trigger branches, including negative
zero, before JSON serialization erases the sign. It pins that NaN in `Order.info`
becomes null in serde_json while TS retains NaN. Independent TypeScript and
Rust status-mapping mutations each failed the named test at exit 101 and were
restored. Additional independent controls changed Rust's price mapping to
`.abs()` and changed the live TypeScript `takeProfit` source mapping to zero;
both failed `normalize_starbase_order_live_typescript_contract` at exit 101 and
were restored. A follow-up Rust mutation that applied `.abs()` only to
`stop_loss` failed the same test at `f64 bits at ok.value.stopLoss`, and a
separate live TypeScript mutation that replaced the `stopLoss` mapping with zero
failed the same named test on the stop-market fixture; both were restored. The
maintained corpus directly asserts extra-field, alias,
ordering, and numeric differences.
Current counts: 258 ported, 155 pending, 13 divergent.

## Private-stream health handler slice

Implemented `markPrivateUserStreamMessageReceived`,
`markPrivateUserStreamDegraded`, and `markPrivateUserStreamAuthFailed` with the
shared production `DefaultStore`.
Their live-source test checks state after each call, notification snapshots,
logs, and forced-warning count. Rust and TypeScript mutations to the
message-received warning-latch reset independently failed
`private_user_stream_health_matches_live_source_and_keeps_notification_finding`
at exit 101; both mutations were restored. The current Store API still returns
unit from notifications, so the three handler rows are `divergent` with
`FINDING` notes covering the TS subscriber-throw ordering. The auth-failed
handler reaches Store indirectly through the degraded handler. The callee-based checker
resolves 142/142 ported callable rows with zero reachable notification chains,
and inventories 13 direct source calls across 10 writer rows (two divergent,
eight pending). The implemented Rust module has two direct Store mutation calls,
both unit-returning, so there are no mutation Results to convert or discard.
Current counts: 258 ported, 152 pending, 16 divergent. The full scoped
`cargo test -p safe-cex-venue-coinbase --jobs 1` suite passes. Adding the
public health state holder also triggered the crate's fail-closed decoder API
inventory; it is explicitly classified there as a stateful non-wire type.
Adversarial review found that the VM oracle initially duplicated the TypeScript
default degraded message. The oracle now reads that literal from the production
source AST; mutating the TS literal failed the named test at exit 101 and was
restored. The reviewer reported no other concrete issue in the slice.
The auth-failed handler's delegated path is also covered for non-empty, empty,
and absent messages. Independent Rust and TS mutations that treated an empty
message as present each failed the same named test at exit 101 and were restored.

## Exchange decimal predicates

Ported `isCoinbaseUnsignedDecimal` and `isCoinbaseZeroDecimal` using
string-only JSON handling and the exact closed ASCII decimal grammar. The live
TypeScript oracle extracts the production regexes and executes both predicates
for absent, null, wrong-type, Unicode digit, BOM, whitespace, sign, leading
zero, fractional, and exponent-like inputs. A Rust mutation rejecting the
unsigned zero spelling compiled and failed
`coinbase_decimal_guards_match_live_typescript` at input `"0"`, exit 101. A
separate live TypeScript regex mutation also failed that named test at exit 101.
Both mutations were restored. The public Starbase parsers are unaffected.
Counts: 260 ported, 150 pending, 16 divergent (426 total).

## Field identity and architecture-dependent NaN audit

Examined 23 maintained dynamic oracle corpora and 11 static TS fixture/evidence
artifacts (34 inputs total) for same-record, same-typed equal fields and
roundtrip-only assertions. The initial corpus scan found 91,874 equal-value
occurrences; the static scan found 28,205, including 28,141 in the unused
`coinbase.products.json` dump. These are pre-repair findings, not remaining
defect counts.
Four crate-local corpora were differentiated in place: `write-edit.cjs`,
`private-health.cjs`, `unions.cjs`, and `wire-cases.cjs`. The original shared
feeder artifacts remain byte-identical. Separate labeled supplements now
exercise error message/data, private order/account/position/trade, write-result
order fields, BTC and ETH instrument, ticker, and captured REST instrument
identity. The six static feeders supplemented are the Starbase RPC, private,
orders, public, ticker-evidence, and instruments-evidence inputs. The
public-trades evidence is consumed only by
the TypeScript fixture test and has no corresponding Coinbase Rust decoder;
it is excluded from claims about Rust decode parity.
The later `rpc-error-response-type.cjs` schema oracle adds a 24th maintained
dynamic oracle after that census. It is Coinbase-local and feeds no external
gate. Its error-response shape has no same-typed sibling fields with identical
fixture values, and Rust assertions read native ID, code, message and nullable
data fields directly.

Both Rust generic wire-check gates had symmetric output assertions: 21 named
legacy DTO checks in `oracle.rs` and 8 cutover type checks in `cutover.rs`.
All 29 recursively read native decoded fields/variants directly on valid cases
before the existing roundtrip assertion, including all nested cutover plan
records. A compiling swap of the two verification fields in
`CoinbaseStarbaseCutoverEvidence` failed `cutover_evidence` at exit 101 on
`credentialedHttpAuthentication decoded field identity`; restoring the mapping
made that named test pass. The supplementary
tests also read native fields for order, health, instruments, ticker, private
records, and error objects. A compiling Rust mapping swap of market
`quote_name`/`base_name` failed `wire_market_response` at exit 101 on the
native `quote_name` assertion; restored code passed the same named test. A
compiling private-order `post_only`/`reduce_only` source swap failed
`write_result_decoders_live_validation_identity_and_serialization_order` at
exit 101 on `shared fixture postOnly identity`; restoration passed the same
target. Both controls operate on cloned fixture inputs; fixture bytes were not
rewritten.

The six NaN-bearing bit corpora are `normalize-order.cjs`, `write-close.cjs`,
`write-edit.cjs`, `write-numbers.cjs`, `write-order-builders.cjs`, and
`write-results.cjs`. `special-numbers.cjs::numberBits` canonicalizes only NaN
to `7ff8000000000000`; signed zero and every finite payload stay exact. The
numeric-ingress and private raw-ingress captures contain no NaN; their corpus
source lists and JSON.parse inputs were inspected. Each bit capture records
both `process.arch` and the Node major in its case or corpus header; the Rust
tests reject a missing or malformed pair. The `write-numbers` corpus previously
spawned Node three times for the same cases; its main test and two controls now
share one `OnceLock` capture, reducing that corpus from three subprocesses to
one. The two corpus controls
`numeric_capture_preserves_signed_zero_bits` and
`numeric_capture_preserves_finite_payload_bits_and_canonicalizes_nan` each
failed by name at exit 101 under a compiling capture mutation and passed after
restoration. These oracle outputs are crate-local; no artifact consumed by an
external gate was rewritten.

The earlier-row follow-up scanned all 21 complete DTO cases in the live
`oracle.cjs` corpus for same-record equal primitive siblings. It found three
groups: two MarketResponse boolean groups already have a one-hot field
identity supplement; all 103 ListAccountsResponse cases had identical
`default` and `ready` booleans. A separately labeled, crate-local supplement
now sets those fields to false and true, leaving the original generated cases
unchanged. The oracle script has no consumers outside this crate. Swapping both
Rust `ListAccountsResponseItem` assignments compiled and failed
`list_account_flag_field_identity_supplement` at exit 101 on
`account.default decoded identity`; restoring the mapping passed. The scoped
`oracle` target passes 34 tests.

Remaining serialization comparisons in cutover blocker/plan tests assert
ordered outputs from an executed operation; they are not decoder-only
roundtrips. The legacy record gate asserts accepted/rejected shapes, and the
13 numeric DTO fields have direct native-field assertions by f64 bits.
Store and request behavior tests compare decoded/constructed native results
against live TypeScript outputs.

The Starbase `isRecord` utility is shared by the RPC and public-frame parsers.
The public-frame live-source corpus adds missing, null, primitives, arrays and
objects, captured in one Node subprocess and compared directly against the
decoded Rust predicate. Swapping the Rust object predicate for an array
predicate and separately mutating the TypeScript predicate to accept arrays
each failed `public_frame_record_predicate_matches_live_source` at exit 101;
the restored `public_frames` target passed all three tests. This corpus has no
same-typed equal-valued field pair and asserts the decoded result, not a
roundtrip. It is crate-local and does not feed another gate.

## Fallible Store notification migration

Integration `9419b340d8a6` changed all notifying Store mutations to return
`Result<(), StoreMutationError>`. Coinbase's three private-health handlers now
propagate that result unchanged: the two direct writes return or propagate
`StoreMutationError`, and auth-failed returns the delegated result. This keeps
the `Notification` variant's Store-local subscriber ID and original cause; no
conversion to `ExchangeError` or local subscriber sidecar was introduced.

The manifest rows for message-received, degraded, and auth-failed moved from
divergent to ported after the subscriber-failure test pinned commit-before-error,
first-failure fanout, skipped later subscribers, and effect ordering. Replacing
the degraded handler's `?` with a compiling `.ok()` discard failed
`subscriber_failure_propagates_after_health_commit_and_before_callbacks` at
exit 101 (`expected original Store notification failure, got Ok(())`); restoring
the propagation made it pass. The message-received path separately asserts its
timestamp/reset and health clearing precede the returned notification error.

## Integration refresh `6fe23f0b3a`

Merged reviewed integration `57c7839dc1` after the normalizer slice. Named
workflow steps were 90 on the Coinbase parent, 90 on integration, and 90 in the
merge. The merged Coinbase `venue-oracle:coinbase` all-target include-ignored
suite passes, including the 30-case order normalizer corpus and the
callee-based Store reachability audit (142/142 ported callable declarations,
zero reachable notification chains, 13 direct TypeScript notification sites
across 10 pending writer rows). Host Clippy and wasm Clippy each pass with
warnings denied; `layers:check` passes 78/78; `parity:check` passes 7,334 rows /
3,228 public Rust items; and extractor check reports 7,334 symbols across 511
files with zero manifest updates, additions, or orphans. The order normalizer
is a one-record mapper with no batch or reduce boundary, so the typed-output
refusal-timing rule does not apply. Coinbase remains at 258 ported, 155
pending, and 13 divergent rows (426 total).

## Subscriber identity and endpoint resolver continuation

At `9af82bfeae`, Coinbase's three private-health mutations propagate
`StoreMutationError` after commit. The recovered `ExchangeError` conversion
appends the Store-local subscriber ID and retains original response metadata.
`subscriber_id_survives_store_to_exchange_error_conversion` pins the path;
removing the shared recovered-path ID append makes this assertion fail at exit
101, and restoring it passes.

`resolveCoinbaseStarbaseEndpoint` maps both transport kinds. Its live TS oracle
supplies both endpoint values and resolver results. Mutating the Rust HTTP URL,
then separately mutating the TS table value, each fails
`starbase_endpoint_table_and_resolver` at exit 101; restoring either side
passes. The all-target `cargo check --keep-going` on the post-cutover crate
completed with zero diagnostics on iteration one; no additional Store sites
were unmasked.

## Public Starbase channel grammar

`isSupportedPublicChannel` is covered by a live TypeScript corpus of 48 channel
strings (32 accepted, 16 rejected), including every public instrument,
ticker/book interval, chart resolution and instrument-state currency, plus
malformed segments, unsupported values, prototype names and Unicode separators.
One Node process returns all cases to the Rust test. A compiling Rust predicate
mutation and a separate TypeScript instrument-table mutation each fail
`supported_public_channel_matches_live_source` at exit 101; restoring each side
passes. No socket dispatch or activation is claimed.

The same one-process capture adds seven `hasExactKeys` cases: ordinary, reordered,
integer-index, `__proto__`/`constructor`, extra, missing and wrong-same-size keys.
Compiling Rust key-count and TypeScript membership mutations each fail
`exact_public_frame_keys_match_live_source` at exit 101; restored tests pass.

## Price and amount precision-axis audit

The production Coinbase TS `placeOrder`, `formatCreateOrder`, and `updateOrder`
remain pending. The Rust venue source contains zero implemented sites that read
both precision steps or round order payloads. Source inventory: `formatCreateOrder`
has one amount-step adjustment and two price-step adjustments (ordinary limit
price and derived stop-limit price); `updateOrder` has three amount-step and two
price-step adjustments. Its creation branch is stop-limit, with no stop-market
branch. Existing inert Starbase builders accept already-specified numeric
values and do not derive or apply market precision. No compiled two-axis Rust
site exists to mutate, so this is a valid negative, not a false-green or a
swap-proof claim. Distinct fractional step-pair cases, direct native payload
assertions, and a compiling swap control are preconditions for porting these
formatters; their manifest rows remain pending and name the finding.

## Expected/native collection cardinality audit

The crate has three `zip` sites. Recursive expected/native comparison in
`tests/support/compare.rs` asserts array lengths before pairing; decoded-wire
identity checks in `tests/support/wire_identity.rs` assert child lengths before
pairing; and `starbase_public.rs` validates all six parallel chart input lengths
before its fixed six-field zip. The latter is an input shape check, not an
expected/native comparison. The private snapshot helper's live-source test
asserts all four TS cases and its native order test independently asserts two
output orders before checking either identity.

Empty-output and subset-output mutations of the Rust snapshot result both
compiled and failed `snapshot_uses_raw_batch_size_and_preserves_map_identity_order`
at the explicit output-count assertion; restoration passed. A cumulative-count
threshold mutation in Rust failed the same named test. A corresponding
production-TS cumulative-threshold mutation failed
`production_typescript_snapshot_cases_pin_boundary_and_error_order` at the
named TS completion count; restoration passed. The relevant oracle runs the TS
corpus in one Node process. No comparison-only zip in the scanned Coinbase Rust
crate was cardinality-blind.

The broader cardinality census covers all 26 top-level arrays emitted by
`scripts/public.cjs` and consumed by the 21 active tests in `tests/public.rs`;
`oracle()` pins each source total. Expected/native arrays use the shared
recursive comparator, which checks exact lengths before pairing. Sequenced and
subscription tests additionally pin every source per-case step-length bucket
and fixed native comparison totals (15,222 and 7,426). The chart mapper has
independent source and native totals (40 array cases / 111 rows) plus a
per-case count derived from the input tick array.

Chart cardinality controls compiled and failed by name: Rust empty output and
subset output failed `chart native rows case 24` (0 vs 3 and 2 vs 3); one extra
native row failed the exact array comparison; source `chartCases.pop()` failed
the pinned 417 source total, and removing one expected row failed the per-case
source row count. To test the zero/zero and source-count-bypass shape, the TS
oracle temporarily returned an empty chart corpus and the source-count guards
were removed; that still failed `native chart-array cases` (0 vs 40). All
mutations were restored and the named chart test passed. This is a targeted
proof for the chart collection path, not a claim that every production mapper
in the crate received four separate production mutations.

The broad public census is inspection evidence, not mutation coverage: the
other public targets rely on pinned top-level totals and exact recursive
expected/native array comparison, but were not each subjected to empty, subset,
source-count-bypass, and extra-output production mutations in this pass.

## Consuming collection decoder audit

The `types.rs` census found 16 public `Vec` fields and two root `Vec` response
aliases across market products, candles, accounts, portfolios, positions,
trades, tickers, level-2 updates, orders, edit errors, and portfolio balances.
The live wire oracle has one native-value identity walker for these decoded
records; it checks exact collection length before walking children, and the
roundtrip comparison is a separate assertion rather than the sole identity
claim.

An injected production decoder on
`PortfolioBreakdownResponseBreakdown.spot_positions` consumed every element
with `next_element` and returned an empty vector. `wire_portfolio_breakdown_response`
compiled and failed at `PORTFOLIO_BREAKDOWN_RESPONSE.breakdown.spot_positions
decoded array length` (0 vs 1). Keeping that exact mutant while removing both
the native decoded-length assertion and the roundtrip assertion made the same
target pass: its remaining direct identity checks do not inspect that array,
and the zip compared zero children. Restoring both guards and production code
made the target pass again.

The permanent `consuming_empty_and_subset_collection_mutants_hit_named_cardinality_guard`
control decodes the same two-element StopDirection array through injected
visitors that consume all input and then retain either zero or one element.
Both are caught at the named decoded-array-length assertion. Removing that
native length guard while leaving both consuming mutants in place compiled and
failed the control test because the mutants no longer failed native identity;
restoring the guard made it pass. This supplements the production-field
mutations above, but does not claim individual consuming mutations for all 18
collection declarations; those remain inspected by the native identity
inventory, not individually mutation-proven.

## Earlier-row audit against PR 7262's pinned corpus

The manifest at `0835fe2e98` parses to 426 rows: 266 ported, 147 pending, and
13 divergent. For price/amount precision, the Rust production census found no
two-axis market-step consumer and therefore zero compiled fields to swap. The
source formatters remain pending: `formatCreateOrder` reads amount precision
for amount and price precision for limit/stop-limit price; `updateOrder` reads
amount precision for current amount, remaining, and amended amount, and price
precision for current and amended price. These pending fields remain a valid
negative census, not a claim that a future formatter is swap-proof.

For decoder identity, the 21 legacy DTO and 8 cutover generic wire checks assert
native decoded fields before serialization; the previous compiling market
quote/base mapping swap failed `wire_market_response` by name. Collection
identity is a separate dimension: consuming mutants on both
`ListOrdersResponse.orders` and
`PortfolioBreakdownResponseBreakdown.spot_positions` failed the native array
length assertion. The two-guard removal control for
`spot_positions` passed with the same mutant in place. The permanent consuming
control is run in the `oracle` test target. Only these two fields received a
production consuming mutation; the 18 collection declarations are inventoried,
not individually mutation-proven.

The live decoder corpora now pin each subcorpus total independently: all 21
legacy wire corpora and all 8 cutover corpora have a named expected case count.
Removing one `CoinbaseStarbaseImplementationStatus` source case compiled and
failed `cutover_implementation_status` at
`CoinbaseStarbaseImplementationStatus: source case total` (7 vs 8); restoring
the source passed the 13-test cutover target. Removing one
`BALANCE_RESPONSE` source case previously failed `wire_balance_response` at
its pinned source total (243 vs 244); restoring it passed the 35-test oracle
target. These source-count controls detect missing oracle inputs; they do not
replace the native-output cardinality checks or claim that all 18 collection
fields have individual production consuming mutations.

## Coinbase credential encoding helpers

`crypto-encoding.cjs` extracts four production helpers and returns all 47
cases in one Node process. The corpus covers both `Uint8Array` and
`ArrayBuffer` base64url inputs, eight UTF-8 strings, ASN.1 lengths around the
0x80 and 0x100 header boundaries plus 65536, Uint8Array tag coercion, and 13
PKCS8/SEC1 decode and error cases. No bit-exact floats or NaNs are present,
and this crate-local oracle feeds no external gate. Rust's
`STANDARD`-alphabet/padding mutation failed
`coinbase_crypto_encoding_helpers_match_live_source` at `base64UrlEncode
uint8-1` (expected `AA`, got `AA==`); restoration passed. The production TS
alphabet mutation failed the same named test at `base64UrlEncode uint8-2`
(`-__w` vs `+__w`); restoration passed. Compiling Rust and production-TS
mutations changing the 128-byte header boundary each failed the same test at
`asn1Wrap length-128`; restoration passed. Compiling Rust and production-TS
PKCS8-to-SEC1 conversion mutations each failed at `pemToPkcs8Der pkcs8-0`;
restoration passed. The UTF-16 lone-surrogate input domain of
`base64UrlEncodeString` remains the chosen shared UTF-8 representation
divergence in its manifest row.

The existing one-process `credentials.cjs` capture also has 13 independent
`parseCoinbaseCredentialJson` cases for BOM trimming, NEL non-trimming, JSON
syntax, non-object values, and returned object fields. A separate source probe
shows that JSON.parse accepts an object containing a lone surrogate; Rust's
serde_json boundary returns no object, the chosen `shared-repr:utf8-string`
divergence. A compiling Rust start-token mutation failed
`credential_json_parser_matches_live_typescript` on the first object case; a
production-TS start-token mutation failed the same test on that case. Both
restored runs passed. This oracle remains crate-local and does not feed another
gate.

### Coinbase credential normalization

`normalizeCoinbaseApiCredentials` is exercised by 19 cases from the current
TypeScript helper in one Node process, including valid PEM and Ed25519 inputs,
nested aliases, JavaScript whitespace, malformed key-name segments and invalid
secret payloads. The Rust PEM-recognition mutation failed
`credential_normalizer_matches_live_typescript` on `pem-secret` at exit 101;
restoring it passed. The TypeScript key-name predicate inversion failed the same
named assertion on `pem-secret` at exit 101; restoring it passed. This mapper
does not import a P-256 key or sign; those operations remain pending on the
shared crypto seam.

## Coinbase spot and CFM REST ticker mapping

`spot-cfm-tickers.cjs` invokes the current `CoinbaseExchange.formatSpotCfmTicker`
body with the current shared `safe-math.round` and returns 25 cases / 26 output
records in one Node process. Its capture header records architecture and Node
major (`arm64`, Node `26.7.0` during this run). Finite values are captured as
exact f64 bits, signed zero included; NaNs are canonicalized to one tag and
nothing else. The Rust test directly reads all
11 native numeric fields before serialization and pins source corpus total,
per-case input/result counts, native output lengths, and 26 actual comparisons.
The corpus is crate-local and feeds no external gate; this code does not decode
any serde collection, so a consuming-deserializer mutation does not apply.

Rust field swaps for `percentage`/`volume` failed
`spot_cfm_tickers_preserve_field_identity_and_cardinality` at the direct
`percentage` identity assertion. Rust `-0` to `+0`, finite `+0.000001`, empty
output, subset output, and extra-output mutations all compiled and failed by
name: respectively the negative-zero `bid` identity, distinct fixture `bid`
identity, `spot CFM native cardinality distinct-0`,
`spot CFM native cardinality two-products`, and the same cardinality assertion
for the duplicate output. With every source-count and per-case-count guard
removed while empty production remained, the independent native-cardinality
assertion still failed on `distinct-0`. A production-TS `Math.abs(price)`
mutation failed the negative-zero `bid` identity assertion. Every mutation was
restored and the target passed afterward. Removing the final source case from
the local corpus failed `spot CFM source corpus total` (24 versus 25); this
corpus feeds no gate outside the Coinbase crate, so the local artifact was
restored in place.

The source intentionally assigns the same parsed `price` to `bid`, `ask`,
`last`, `mark`, and `index`, and the same zero literal to `fundingRate` and
`fundingTime`. Their equality is part of the source behavior, so swapping only
within either equal-valued group is observationally equivalent for every input;
the direct native assertions still pin each field against its source value.

## Coinbase Starbase public test error parser

`public-test-error.cjs` executes the current `parsePublicTestError` and its
`isRecord` dependency once per corpus. The 19 pinned cases cover exact keys,
safe-integer boundaries, non-empty message validation, prototype-named extra
keys, and absent, null, and object-valued `data`; native fields are checked
directly, preserving the absent-versus-null distinction. This corpus is
Coinbase-local and feeds no external gate. Removing Rust's safe-integer bound
compiled and failed `public_test_error_matches_live_source_and_preserves_optional_data`
on `unsafe-integer`; changing the TypeScript predicate to `Number.isInteger`
failed the same assertion on that case. Both mutations were restored.

## Coinbase Starbase public frame context

`validate-public-context.cjs` executes the current source validator and its
channel tables once for 19 cases. It pins supported and duplicate-channel
checks, positive safe-integer request IDs, duplicate IDs, and validation order
(`acknowledged`, `desired`, IDs). Swapping the first two checks in Rust failed
`public_context_validation_matches_live_source_order_and_cardinality` on
`acknowledged-error-precedence`; swapping them in the TypeScript source failed
the same case with the opposite expected error. Both mutations were restored.

## Coinbase Starbase JSON-RPC error response type

The single-process rpc-error-response-type.cjs oracle derives the full
TypeScript schema for the error-response alias and nested error object. Rust
schema reflection and direct value assertions pin the 2.0 singleton, optional
request ID, nested code/message identity, and absent-versus-null error data.
Removing the Rust ID omission attribute compiled and failed the named test at
the absent-ID omission assertion; making the TypeScript ID required failed the
source-versus-Rust schema assertion. Both mutations were restored. The
CoinbaseStarbaseJsonRpcVersion enum is an exempt helper for the inline
singleton literal, not an exported TypeScript symbol.

## Coinbase Starbase WebSocket disconnect reason

`ws-disconnect-reasons.cjs` extracts both source type aliases' complete literal
unions and the policy object's field shape in one Node process. The Rust public
`Deserialize` implementations first require a JSON string, then match only
their declared spellings. The qualified public decoder test covers all 3
disconnect reasons and all 6 session states; it rejects wrong-case, whitespace,
empty, primitive, array, and externally tagged object forms. No permissive
inherent decoder or conversion path is exposed. The policy is serialize-only:
its optional scope is omitted when absent, and it has no runtime TS decoder.
Removing the Rust omission attribute compiled and failed the named target at
`absent optional scope serialization` (`null` versus absent). Making TS `scope`
required independently failed `source policy declaration shape`; both
mutations were restored.
Replacing the strict implementation with a derived enum decoder compiled and
failed `websocket_session_literal_unions_match_closed_typescript_sets` on
`{"heartbeat-timeout":null}`. Adding a TypeScript union member independently
failed the same named test at the pinned source accepted-spelling total (4 vs
3). Both mutations were restored and the target passed after restoration.
The same two controls were run against the session-state union: a derived
decoder mutation failed on `{"idle":null}`, and a TypeScript-only added state
failed the pinned source accepted total (7 vs 6); both were restored.

## Review correction: wire identity and cardinality

The 21 legacy and 8 cutover generic wire checks now pin the number of complete
samples whose native identity closure runs (per type, before roundtrip output is
examined). Renaming the generated `: complete` label compiled and failed the
named `native identity assertion count` at exit 101; restoring the label passed.
Fourteen enum-typed DTO fields now match Rust variants directly: INTX and CFM
position sides, trade side, Level2 side, three order-event fields, three list-
orders fields, place-order side and margin type, and both portfolio side fields.
A symmetric BUY/SELL serde rename compiled and failed
`LIST_ORDERS_RESPONSE.side native BUY variant` at 101; restoring it passed.
The direct variant control is separate from re-encoded-byte equality.

Cardinality is independently pinned at each reviewed site. Capture metadata
expects 19,602, 22, 30, 242, 36, 2,888, 1, and 1 headers in its eight consuming
test targets. Removing capture headers failed `capture metadata header count`
at 101, and restoration passed. The public-frame artifact has 66 total rows,
with fixed subsets of 48 channel, 7 key, and 11 record cases; emptying it failed
`public-frame source corpus total` at 101, then restoration passed. The endpoint
table asserts exactly two entries and their direct HTTP/WebSocket identities;
emptying it failed `HTTP and WebSocket endpoint count` at 101, then restoration
passed. The Store audit independently counts ported callable rows and matching
source declarations (151/151); it inventories 13 direct writes across 10 rows.
Forcing the source-declaration count to zero failed
`source declaration census matches ported callable rows` at exit 1, and the
restored audit passed. These counts are from the current reviewed branch state.

## Review follow-up: independent count floor and native field identity

The Store reachability checker now pins its independent ported-callable corpus
at 151, separately requiring all 151 source declarations to resolve. Changing
all manifest rows to pending failed at `ported callable corpus total` (0 versus
151, Node exit 1); restoring the manifests reports 151/151 and zero reachable
notification chains. A compiling production mutation swapping Ticker `price`
and `volume_24_h` failed `TICKER_EVENT.tickers[0].price decoded field identity`
at Rust exit 101; restored `wire_ticker_event` passes. Direct decoded-value
checks also cover both Market display symbols and all Ticker fields. The generic
enum projection diagnostic is named as serialization projection, not native
variant identity.

At merged state `5bb2ef089810`, the branch has 267 ported / 13 divergent / 146
pending manifest rows. Full scoped Coinbase tests, strict host Clippy, strict
wasm Clippy, workspace formatting, extractor drift (7,341 symbols, 0 changed,
0 added, 0 orphaned), and layer audit (78/78) passed. Parity check reports an
unchanged AsterDEX source/manifest finding; its paths are byte-identical across
the Coinbase merge and outside this slice. TOML exemption parsing reports 371
blocks / 371 distinct entries.

## Legacy API credential normalization

After merging integration `750c6b3ed5386e39fb74893af828ac1d55644648`, the
Coinbase manifest measures 269 ported / 13 divergent / 144 pending; the merge
preserved 92 named workflow steps from each parent and in the result. This slice
ports `parseCoinbaseCredentialJson` and `normalizeCoinbaseApiCredentials`.
The maintained TypeScript subprocess emits parse and credential projections in
one invocation; its normalization cases cover object credential wrappers,
name precedence, JS trim including BOM, PEM key labels, ED25519 prefix/base64
lengths and both exact failure messages. A Rust key-name case-folding mutation
compiled and failed `credential_normalization_matches_live_typescript` at 101;
restoring it passed. The parse oracle pins all nine inputs and its six-null,
three-object outcome split; normalization independently pins every key/secret
pair (54 total) and its 15 success / 39 error and ES256 / EdDSA branch counts.
A same-length key substitution compiled and failed
`normalization key case identity` at 101; restoring the TypeScript generator
passed the targeted test. The 66-row public-frame oracle now pins all 48
channel values, seven key records, and eleven record-predicate values. Replacing
the same-length invalid SOL symbol with BOB compiled and failed
`channel corpus case identity` at 101; restoring the generator passed. Exemption
tomllib parsing reports 375 blocks / 375 distinct entries.


## Coinbase concrete snapshot native error bridge

Source proof: `b654fe814138b5069b22e6246f879f9aad537ac5`.
Integration merge: `b575df520a974c61587111341dc290fbb7722f67`, incorporating
origin `53ee502232b005173d8a6f3f0d7cf94bad7f072c`. The working parent was
`a58f9ce8f1650bfb3d88347cb3e9071ab39b44ae`; its two parents were the previously
submitted `f6343d7510c8dd7e74c20a3d8918aba525cb3a42` and origin 1810. Both
previous work and the integration batch are merged ancestors; this slice has
local sub-agent reviews and has not been pushed.

The tomllib census is 393 ported / 33 divergent / 0 pending across sixteen files,
stopping at `coinbasespot.exchange.ts.toml` (426 exact row identities). Against
origin 53ee, one divergent/FINDING row moves to ported, zero pending rows move,
and no row is added or removed. Its three existing native targets are retained.
Older origin 1810 already had the row ported before the class carrier withdrawal
reached integration; no status gain is claimed against that older revision.
Sixteen total FINDING rows remain, fourteen involving notification paths. This
repair closes the snapshot's native error-carrier defect; unrelated runtime,
error-property and raw-event bindings remain findings. Classification follows
[the canonical taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy).

At source revision b654, native `class_private` (two tests) plus
`decoder_boundaries` (one test) exited 0. The Node notification audit exited 0
with fourteen tests, all 262 reviewed roots retained, 248 supported callables,
fourteen acknowledged notification roots, 29 call paths rooted in findings and all thirteen
content-pinned direct writes. These results are not workspace build claims.

Three production mutations compiled and failed the named native assertion at
exit 101, then each restoration passed at exit 0:

- Drop the native Store origin: `class snapshot native Store notification origin retained`.
- Swallow the class publication failure: `class snapshot notification failure escapes dispatch`.
- Publish the wrong loaded flag: `class snapshot native loaded committed inside subscriber`.

There were no surviving controls. Native assertions inspect the actual class and
DefaultStore values, original subscriber/cause pointer and failure root before
source wire projection. The finite six-scenario source floor ends at snapshot
notification recovery. The typed Store witness uses a fixed imported Store and
keeps the class Frame/order roots separately; it is not a full exchange runtime
or live Store-replacement proof. The eight source/oracle input hashes are pinned.

The merged tree's full Coinbase `--tests` command exited 0: 53 completed
targets and 264 observed tests, zero ignored/filtered, stopping at
`ws_disconnect_reason`. The target expectation comes from the library plus
fifty-two direct integration-test files. A scoped `git diff --exit-code` from b654
to b575 exited 0 for Coinbase source/tests/scripts, Store, native subscriber-error
and all six maintained TypeScript inputs used by the class oracle. The exemption
conflict kept both helper groups: 1,802 unique entries, zero duplicates.

Receipts, exact commands and sanitized logs are tracked in
[evidence/snapshot-native-notification](evidence/snapshot-native-notification/README.md).
The manifest writer was reconciled before the source commit and printed
`Updated 0 manifests; 0 added, 0 orphaned symbols.` (exit 0); that is not a final
extractor-check result. Two pre-commit fixture compiler errors were corrected
before b654; they are not mutation-control failures or passing gate results.

The three required shipping gates run after this evidence commit: workspace fmt
check, locked workspace build and host workspace all-target clippy. Their exact
handoff SHA, commands, exits and logs are saved locally in
`.aimux/coinbase-notification-handoff/<handoff-sha>/gates.json`; source-revision
receipts above do not replace those measurements. NOT RUN here: canonical wasm
clippy, workspace check/full workspace tests, extractor check and the central
wiring/layers/routing sweep. The user owns wasm and the central sweep. Remote
parity-job green remains unconfirmed because integration jobs are cancelled.

The retained-info cutover executes `transport_binding::retained_info_native::native_http_and_store_keep_opaque_coinbase_info`
against the production NativeRuntime, reqwest HTTP host, Coinbase binding and
DefaultStore. Maintained `CoinbaseExchange.fetchMarkets` supplies the expected
native observations. Overflow infinities, negative zero and lone UTF-16 pass
through actual HTTP; native NaN, own undefined and array values then pass through
the model codec and Store. The same test compiled against archived master
`2e703b7b8ee3a2a083e63530bde1a657b340f601` fails product admission (0 versus 1)
after the old JSON projection rejects the opaque lone surrogate. The retained
implementation passes. The observer reads numeric bits, UTF-16 units and own
field counts before any JSON output.
