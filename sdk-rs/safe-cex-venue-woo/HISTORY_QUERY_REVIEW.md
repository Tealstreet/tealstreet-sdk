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

## Retained history diagnostics

Both maintained catches (`woo.exchange.ts:1504` and `:1579`) pass
`err?.response?.data?.message || err?.message` to BaseExchange.emitAndLogError
and resolve an empty array. The native helper now selects the retained response
message before converting it directly to the runtime value. Fallback uses the
canonical error projection. There is no JSON or UTF8 message conversion.

The constructed-class regression drives the real clients, signers and REST adapter
for both public fills and internal trades. Thirteen profiles per method cover
numbers, booleans, objects, arrays, nested message fields, isolated D800/DC00 units,
and falsy or missing response messages. The reference executes the maintained
catch blocks and BaseExchange reporter. Native event strings are compared as
UTF16 units, with exactly one event, empty results and signed request counts.

On master e059bc2008, both named tests compiled and failed: the numeric response
rejected instead of resolving, and isolated surrogates became FFFD. The same
26 profiles pass after the reporter cutover. The existing history/query corpora
keep every source case and assertion; their error fixture now stores the response
on RestFailure instead of the removed context-based response override. These diagnostics close one live defect in audit rows 19–20;
parser/output obligations and the rest of the sixty-row audit remain open.
