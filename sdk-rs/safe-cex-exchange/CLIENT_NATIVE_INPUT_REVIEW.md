# Client native input review

## Header name conversion

`js_runtime::client::normalized_header` is FIX: the installed Axios
`lib/core/AxiosHeaders.js` implementation evaluates
`header && String(header).trim().toLowerCase()`. Calling the input's `toString`
method directly rejected primitive results other than strings and skipped the
`valueOf` fallback. The native helper now uses canonical string conversion before
trimming and lowercasing. Falsy input still selects the existing empty name.

`tests/js_runtime.rs::client_header_name_coercion_matches_installed_axios`
compares the actual native AxiosHeaders facade against installed Axios for twelve
get/has profiles: string and number returns, fallback, a noncallable `toString`,
an original thrown object, and failed primitive conversion. It checks hook order
and original thrown identity. Restoring the original helper produces a compiled
failure for the numeric return; restoring the fix passes the same comparison.
The existing four source profiles and seventeen set operations remain unchanged.

The canonical accepted contract calls the private production helper for nine
scalar/UTF-16 inputs. Its outputs come from executing the installed Axios helper
in `tests/js_headers_oracle.cjs`. They cover lone UTF-16 units, expanding Unicode
lowercase, negative zero, number, boolean, null and undefined. This closes one
of the 31 pending client owners; the other 30 retain their pending status.
