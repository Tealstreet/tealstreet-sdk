# GRVT field-identity audit

Baseline e03504cacb: 22 test/oracle files. The checker-generated record census
examined 23 top-level records, 190 primitive leaves after nested expansion, and
found 26 equal-value groups in 21 records. Existing per-field numeric cases
already read native fields; the general record checker only roundtripped bytes.

The in-crate record oracle has no external reader found by static reference
search. It is repaired in place: 187 added one-field cases, 1,595 to 1,782 total,
with the same 264 lone-surrogate refusals. The `--identity-audit` mode prints the
census; its 263 accepted nested-or-union cases include pre-existing cases and
must not be described as 263 new identity cases. No generated census is committed.

The records gate now reads 182 native fields across 25 structs (23 top-level plus
two nested). Explicit wire keys in the test remain independent of Serde renames;
exhaustive destructuring makes a new field require an audit. Native scalar values
are asserted directly, including f64 bits; opaque buffers compare the selected
native field's JSON-data content, without serializing the containing record.
The union gate now matches native enum variants to explicit literals before its
wire check. Two gates converted from roundtrip-only assertions.

Compiling production controls swapped GrvtOrder.post_only/reduce_only,
GrvtAuthRequest.api_key/signature, and OrderLeg.instrument/size. They failed
`GRVT native bool identity` or `GRVT native string identity`. A symmetric rename
of both public Serialize and private Deserialize post_only/reduce_only fields
also compiled and failed `GRVT native bool identity`. All were restored to green.

| Owned gate | Assertion shape and identity evidence |
| --- | --- |
| records | Direct native fields plus retained wire/shape check; repaired above |
| types | Direct native variant plus literal/accepted-set check; repaired above |
| numeric_records | Direct per-field bits through six entries; WS opaque fields additionally receive one-field cases in records |
| protocol, builder_protocol | Named table entries/order and explicit network selection |
| builder_helpers | Native address/fee/Error output; scalar fee and network varied |
| builder_info | Native builder/fee text/fee bits; 324 combinations plus distinct Unicode contexts |
| builder_error | Native caps, reason, name/code/message/display; independent cap cross product |
| order_semantics | Native TIF and trigger fields/bits; alias and flag cases vary independently |
| balance_prototype | Actual-source state effects in fresh realms; not a DTO identity gate |
| census_heritage | Named fail-closed source controls; no runtime record fixture |

No GRVT fixture feeds another crate in the static reference census. Existing
candidate evidence stays historical; it is not a shipped regression guard. NaN
capture policy is in safe-cex-num/tests/NUMBER_CAPTURE_AUDIT.md; feeder
preservation is in safe-cex-num/tests/FIELD_IDENTITY_AUDIT.md. Venue row classifications do not change.

The divergent-row review found that order_semantics had projected TIF/trigger
enums and flags through Serde despite the table above. The follow-up now checks
all14400 native TIF results and10998 native trigger results, then the boundary
refusals, before the separate wire passes. This corrects that earlier scope claim.
