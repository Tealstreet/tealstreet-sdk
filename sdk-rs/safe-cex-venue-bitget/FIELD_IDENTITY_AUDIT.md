# Bitget field identity audit

Scope: 12 frozen JSON artifacts, their live producers, and 13 Rust integration
test files. `scripts/field-identity-audit.cjs --pairs` enumerates sibling scalar
pairs by wildcard path, including tagged numbers. It deliberately includes
metadata, input/output echoes and protocol aliases: its candidate count is not
a defect count. Review the semantic use before changing a value.

The fixture search found no tracked basename consumers outside this crate.
`types-oracle.json` also supplies this crate's private category inventory test.
The shared numeric oracle reads the actual TypeScript income `num` function;
it does not consume these fixture files. That scalar wrapper is not a multi-field
fixture. No outside-feeder artifact is rewritten by this change.

## Assertion shape

| Test target | Existing observation | Identity supplement |
| --- | --- | --- |
| types | Nine type arms decode and re-encode; five timestamp leaves also read native bits | Explicit native members of 17 records / 178 fields; eight record roots, raw/owned/borrowed; three category variants also checked directly |
| ticker | Explicit ordered native field pairs and scalar results | Existing `all-output-order` uses distinct numbers; signed-zero-all-fields intentionally equal |
| ticker_handler | Explicit native Ticker members after actual Store mutation, plus notifications | Distinct 11-field seeds; unchanged, partial and complete update for both handlers |
| order | Native numeric members and source-produced record comparisons; not symmetric wire decode/encode | Distinct linear/inverse order inputs and position-fold fields |
| fill_helpers | Explicit native execution/income members | Existing source field selection and coercion cases; scalar, direction and fee aliases have distinct source cases |
| bill_page | Explicit native income fields, request params, error identity and call order | Numeric capture probes preserve finite and zero identity |
| history_page | Native read result, cursor, errors and read trace | Existing source comparisons; no decoder roundtrip claim |
| income_history | Returned native numeric members, page identity and reporter trace | Page/sameOpts equal booleans are independent trace assertions, not record fields |
| plan_selection | Native returned selection and resolver/selector call trace | Frozen artifact has no always-equal sibling pairs; separate inline Position seed repaired with nine native fields on store and resolved paths |
| order_lifecycle | Native ordered cache entries and time/prune traces | Same-valued fixture pairs also have distinguishing cases |
| pure | Separate source predicates and native outputs | UTF-16 witnesses intentionally make several predicates true; positive scalar cases distinguish predicates |
| history_reference | Source snapshot only | Does not claim Rust decode identity; fill_helpers consumes source cases independently |
| signing_reference | Source preimage/wire snapshots; native toJsonString compared separately | Does not claim Rust createAPI decoding parity |

A re-encoded snapshot remains useful for serialization and acceptance checks.
It is no longer the only wire-record identity assertion. The new native mapping
is explicit test code; it is not read from Rust's serde names. Exhaustive Rust
destructuring and the source field inventory refuse unaccounted fields.

## Candidate classification

The initial full JSON walk found 7,007 equal sibling-pair paths, 3,676 always
equal, over 403,373 objects. Most wire-shape candidates repeat the same 17
records through nested positive and negative cases. Schema descriptors, table
aliases, source input/output echoes and UTF-16 refusal metadata must not be
counted as independent field-identity repairs.

The supplements address wire-record and category identity, zero-valued ticker
seeds, preserved position-fold fields and async selector callback fields. The
order supplement also differentiates primary quantity/price/filled/attached-leg
fields where existing tests already detected the measured mapper swap. Frozen payloads remain unchanged; supplements
make no retroactive independence claim. The bill-page header change belongs to
the separate NaN architecture repair and leaves all original cases unchanged.

The 3,676 always-equal candidate paths reconcile as follows:

| Group | Paths | Classification |
| --- | ---: | --- |
| Type wire samples | 2,910 | Distinct supplement plus direct native fields across all 17 records |
| Type protocol aliases and generation labels | 589 | Equal by source contract; direct named table checks retained |
| Type acceptance metadata | 1 | Value/valid echo, not decoded record fields |
| Ticker seeds | 55 | All 11 numeric fields differentiated, including fields untouched by the handler |
| Ticker write trace | 1 | id/key stringification identity, not two native fields |
| Order corpus | 108 | Semantic breakdown below |
| Other corpora | 12 | Independent predicate/refusal metadata, category presence/identity, page-call identity and scalar preservation |

The remaining 3,331 equal-pair paths also have differing values elsewhere in
their corpus. Native-output assertions and the operation's observed inputs were
reviewed separately; merely finding a differing value is not a parity proof.

The 108 order paths comprise 14 position-fold paths (new distinct/native checks),
12 status alias paths (4,096 conflicting combinations already distinguish
precedence), 28 UTA/classic position paths (distinct baselines and single-field
probes), and 24 inverse/reference/numeric-info paths (alias probes plus explicit
reference/non-finite witnesses, strengthened by distinct attached-leg inputs).
Eight cancellation paths contain unread close/reduceOnly or position defaults;
the source reads type, amount, symbol, side and info.planType here. Nine
sizing/liquidation/PnL/market paths concern fields unused by those operations;
relevant contracts, entry price, liquidation price, identity and side already
vary independently. Thirteen remaining paths are preservation checkpoints,
UTF-16 witnesses and independent source-result metadata. Equal source aliases
and preservation results are retained, not rewritten to invent a distinction.

An independent inline scan covered all 13 integration test files plus two support
files, finding seven repeated-value seed sites in order.rs and plan_selection.rs.
Six have distinguishing cases, stable original-pointer assertions, or fields
outside the operation's observations. The seventh exposed Position fields passed
to the async selector but never observed by its fixture callback. Its supplement
now reads nine native fields on both the store and resolved paths, including two
resolved positions. Source inputs are snapshotted BEFORE execution so source
mutation cannot change both the test input and its expected output unnoticed.

Repair scope: five identity groups across four test targets (wire records and
category variants in types, ticker seeds, fold input fields, async selector
fields). Two additional linear/inverse mapper cases strengthen existing coverage;
the price/amount swap already failed an older case and is not counted as a newly
found defect. One previously roundtrip-only test target now directly checks all
nine source type declarations. Selection-origin and callback tests also gained
native-field checks; their previous assertions were selection evidence, not
wire-field identity evidence.

## Compiling controls

- Required locked/available serde-name swap: old frozen types test exits 0;
  new test exits 101 naming FETCH_SINGLE_ACCOUNT_RESPONSE/data/locked.
- USDT/USDC serde-name swap: old frozen types test exits 0; new test exits 101
  naming native category identity USDT-FUTURES.
- Ticker bid/ask destination swap: exits 101 naming field-identity:all.
- Order price/amount swap: existing unknown-type-strategy exits 101.
- Async selector entry_price/notional swap immediately before callback: old
  frozen_plan_selection exits 0; supplement exits 101 naming plan native
  position field identity entryPrice 0. Independent actual TypeScript mutation
  at the same callback boundary also exits 101 at that named assertion.

Every control was restored and its targeted test passed. This is an audit of the
current owned fixtures, not a claim about future additions or every possible
conditional corruption. No external feeder was changed; frozen identity artifacts
remain byte-identical and separately labeled supplements make no historical
independence claim. The NaN metadata-header repair is separate.
