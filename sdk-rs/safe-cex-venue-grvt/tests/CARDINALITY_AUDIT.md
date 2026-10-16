# Collection cardinality audit

Scope: the five typed vector fields, three opaque WebSocket payload fields and
five protocol tables in GRVT's implemented wire surface. Private wire companions
duplicate field types but are not additional outputs. Scalar-source corpus
completeness is separate from production collection-output coverage; the num
audit and the supplementary source-array section below state their own scope.

| Record | Native field | Empty control target | Partial control target |
| --- | --- | --- | --- |
| GrvtOrderBook | bids | original record gate | cardinality supplement |
| GrvtOrderBook | asks | original record gate | cardinality supplement |
| GrvtAccountSummary | balances | original record gate | cardinality supplement |
| GrvtInstrumentLite | v | original record gate | cardinality supplement |
| OrderPayload | legs | original record gate | cardinality supplement |

Each production mutation runs after the private decoder consumes the entire
record, then clears that one field or truncates it to one element. Thus trailing
input rejection cannot substitute for the native assertion. Each field is
mutated independently; a failure on bids cannot stand in for asks or balances.

The original 1,782-row `records.cjs` corpus is unchanged. Its native identity
check compares each vector's length before zipping elements or serializing the
record. Empty controls measure coverage now, after the earlier native-identity
repair; they do not establish that the pre-identity-sweep gate was never blind.

The supplement selects exactly one TypeScript-admitted all-fields fixture for
each field, verifies the original single element, and repeats that element to
make two. This is labeled derived evidence, not a new independent TypeScript
capture or field-swap witness. The native identity check observes the two-element
input; an additional returned-field count requires two, and the completed field
count requires five. Original feeder bytes are not regenerated.

All ten mutations compiled and exited 101 at `GRVT native array length`, with
the relevant record and field appended to the assertion. Production was restored;
the scoped supplement then passed (one test, zero failures). Five empty controls
were caught by the existing gate; five proper-subset controls use the new derived
supplement. No original fixture values changed and no never-blind claim is made.


## Protocol tables

Each of BASE_URLS, WSS_URLS, ENDPOINTS, GRAFANA_ENDPOINTS_EVERY and
GRAFANA_ENDPOINTS_PERIODIC was independently replaced by an empty slice, then
by a slice missing its first entry. All ten constant-table mutations compiled
and exited 101 at `GRVT protocol table equivalence`. The unchanged original gate
pins 44 source entries and compares the full native rendering. These are
constant-table controls, not runtime collection-return claims. Production was
restored and the scoped protocol target passed.

## Opaque WebSocket payloads

The new structural supplement supplies two-element string arrays and two-member
string objects to params, result and error.data. It does not replace the source
checker corpus or claim independent TypeScript capture. `record()` compares each
complete opaque value before its outer record serialization; the supplement also
requires two returned members per field and twelve completed member checks.

Each field was independently cleared or reduced to one array element/object
member AFTER full record decoding. All twelve mutations compiled and exited 101
at `GRVT native opaque field identity`, naming the mutated field. Payload strings
avoid replacing the cardinality proof with a numeric-projection refusal. These
controls test present coverage with a new structural supplement, not a historical
never-blind claim. Production was restored and the scoped supplement passed.

Two additional controls retained the params production mutation while bypassing
its existing opaque identity assertion. Array-empty and object-subset controls
still compiled and exited 101 at `GRVT-OPAQUE-CARDINALITY returned count /params`.
Both the identity assertion and production decoder were restored; the scoped
supplement passed again. The two layers therefore have independent failure proof.

## Supplementary source arrays

Interval UTF-8 refusals (3), instrument-symbol refusals (2), signing-metadata
refusals (2), and trigger boundaries (1) now have their own exact source totals
and completed assertion counts. They use num's test-only cardinality walker,
without modifying any original oracle. Its permanent driver has a separate
`--group grvt` selecting only these four targets. This supplements source-array
completeness; it does not widen the earlier production-collection proof scope.
