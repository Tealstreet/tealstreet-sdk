# Retained history parser

The constructed WOO history members previously projected the complete REST response
to serde_json::Value before selecting its rows, then parsed each row through that
same closed view. An unused surrogate-valued property could therefore discard an
otherwise valid fill. A selected numeric string containing a surrogate threw at
projection instead of becoming NaN and failing the source row-validity guard.

The maintained source at woo.exchange.ts:472–579 selects exact/snake/camel fields,
uses Number for numeric operands, and selects nested rows, root rows, nested data
or the root array. Empty arrays are present. History candidates at lines 597–678
share that selector; both historical callers parse all rows before filtering.

Both primary and legacy reads now carry RetainedJsonInput directly through that
selector and the transaction parser. The class glue converts retained data directly
to/from native values. Number and truthiness use the shared retained primitives;
own undefined stops an exact key hit before the caller's nullish fallback. Original
query names, signatures, candidate ordering, row ordering, sorting and slicing remain.

The real signed-class regression covers 64 maintained-source profiles: both callers,
primary and legacy response paths, four envelope shapes, ignored envelope/row UTF16
metadata, selected invalid numeric text beside a valid fill, and exact own undefined
shadowing a snake-case numeric value. It checks every native result key, binary64
numbers, visible errors, signed request count and nonempty signatures. The source
executes the maintained selector, parser and historical member bodies. Its existing
14 null/array/primitive cases remain intact.

The final regression was compiled with all seven production source files restored
to starting master 2b2cf9a560. It failed the named signed-history assertion (exit 101,
empty native result versus one source fill). Every fixed production byte was then
restored exactly. This is a real baseline failure, not a compilation refusal.

The original 272 transaction cases still compare 2249 native fields, including
number bits and owned commission-asset observations. The 487 response cases retain
the four explicit native-NaN exclusions and the selected-array pointer assertion.
The primary 72-case and legacy 90-case corpora retain their independent request,
row, error and boundary counts. Historical caller/output corpora are unchanged.

This batch fixes raw response/row admission. It does not claim closure of the
remaining strict historical String/output and dynamic-option boundary review;
FIX_AUDIT_LEDGER rows 19 and 20 remain open for that reconciliation. Class row 60
is separate. No source TypeScript behavior, shared model or fallback path changes.
