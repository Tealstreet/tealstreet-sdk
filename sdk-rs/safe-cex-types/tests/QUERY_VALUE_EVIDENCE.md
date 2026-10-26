# Request-side query carrier

QueryValue is an explicitly constructed request-side value. Undefined and null
are separate variants; Number retains native f64 including NaN, infinities and
signed zero; strings such as "-0" and "1e400" remain String. Arrays and ordered
objects recurse. No Display, serde Serialize/Deserialize, from_value or JSON
projection is supplied. Existing retained_json JSON NaN refusal is unchanged.

query_value_profiles.json preserves the six actual-source witness rows: three
inputs on KucoinSpot indexed and BinanceSpot repeated-array qs profiles. Each
row records source expression, qs version, entrypoint SHA-256 and capture
architecture. Input tags retain undefined, NaN and signed zero without platform
NaN payload bits. Null renders an empty value; undefined is omitted. Nested and
empty arrays/objects are included. This measures qs query rendering, not Axios
form-body toURLEncodedForm/URLSearchParams. Core must carry the serializer profile
per call site alongside params; a global indexed/repeat choice is incorrect.

A JSON-projection mutation in the source witness executes and fails the named
QUERY-VALUE source NaN query spelling assertion; restored native carrier test
passes. compile_fail checks refuse accidental Serialize and Value.into paths.
The Num request_query_values_cannot_enter_output_eligible_records trybuild gate
pins the compiler refusal for an output record embedding QueryValue. It has no
JsNumberOutput implementation and must not become an output-check bypass.

Only the carrier and witness are supplied here. Core owns RestRequest, signing,
secondary forwarding, actual qs-compatible renderer and its pipeline controls.
No HTTP behavior or pipeline adoption parity is claimed by this contract.
