# Historical query values

The assigned `fetchHistoricalFills` and `fetchHistoricalTradesInternal` rows
contained a reachable query loss: `Math.min(Math.max(NaN, 1), 500)` is NaN in TS.
The maintained methods send that number through their signer before slicing to
an empty result. Rust's JSON parameter conversion rejected it before any HTTP.

Both V3 history requests and the legacy candidate fallback now use only
`RestRequest.query_params`, with an explicit indexed, encoded, insertion-order
profile. `JsRestAdapter` restores that canonical QueryValue directly into the
native Axios config before the actual interceptors/signers. JSON and native
parameter carriers together reject visibly before any client call. No extension
property, string substitution or alternate history path is used.

Restoring the exact three production files from master `c0812bef8e` makes all
four new constructed-binding tests compile and fail by name. The public caller
sends zero terminal requests instead of one; internal history sends zero instead
of ten; native query values are dropped, and conflicting carriers reach HTTP.
Exact fixed-byte restoration passes all four tests. The public history caller
and internal/legacy class members retain their original effect ordering. Their final URLs and HMACs match
the maintained TypeScript history helpers and actual signV1/signV3 bodies,
using each observed request's timestamp. This compares signing independently of
the terminal HTTP harness and retains rate-limit advancement between requests.

A separate actual request interceptor observes all eight QueryValue variants,
nested arrays/objects, own undefined, null, booleans, text, signed zero, NaN and
both infinities before signing. It checks native number bits and property
presence before the wire comparison. The conflicting-carrier witness requires
zero HTTP and preserves the original TypeError through the adapter's rejection.

The original source corpora and fields remain. The two previously excluded
NaN-limit caller cases are now compared: 46 of the existing 48 cases, 47 logical
fetches, 182 REST attempts, 61 outputs and nine logs. The other history/legacy
corpora retain their original request/error-order floors; all 28 native numeric
parameter cases now transmit, including seven previously refused NaN fields.

Truthy non-string history error messages remain a separate OPEN repair using
the canonical retained-response accessor; this query change does not claim to
resolve that diagnostic row or the full sixty-row WOO audit.

## History page row admission

`woo.exchange.ts:472–476` calls the shared `v(row, 'symbol')` before any row
validation. `utils/get-key.ts:8–13` returns undefined for null and missing array
properties, but throws the original `in`-operator diagnostic for primitives.
The old Rust object-only guard discarded the entire result for a null/array row
and replaced primitive diagnostics with `history parser requires an object row`.

`history_row_tests::` reaches both configured history methods through the real
signed HTTP adapter. Fourteen source/native profiles place null, empty/nonempty
arrays, empty objects, numbers, booleans and strings before a valid fill. The
maintained TS methods and parser supply the expected rows and diagnostics; the
native observer checks exact own keys, number bits (including negative-zero fee),
report text/count and one signed request. Both named tests compiled and failed on
master e059bc2008 before the shared get-key admission was used.

Rows 19/20 stay OPEN: this bounded repair does not clear the retained row/string,
option, parser-member or output representation obligations. The error reporter
repair is tracked separately in #1089. No existing corpus case or assertion is
removed, and the full public-history API is unchanged.
