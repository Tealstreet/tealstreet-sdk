# Phemex Unknown outcome telemetry

Phemex opts into the engine's immutable Unknown-outcome hook. Engine-sealed
HTTP and transport failures publish one `order.outcome_unknown` event at terminal
completion, including frame-fed operation. Venue-decoded Unknowns use the same
projection from their response path. Late callbacks publish nothing further;
definite refusals publish no Unknown event. Reporting changes no dispatch,
classification, retry, store, reconciliation or protective put-back authority.

The projection matches NX #9262 and emits only these keys:

| Field | Contract |
| --- | --- |
| `command` | `phemex.<command ID>`, an ASCII label of at most 64 characters |
| `request` | Internal decimal request ID, matching `^[0-9]{1,20}$` |
| `outcome` | Literal `unknown` |
| `placedOrderIds` | Comma-separated decimal IDs of 1–32 digits; at most 16 |
| `unknownIndices` | Comma-separated known Unknown indexes in 0–65535; at most 16 |
| `status` | Optional numeric HTTP status in 100–599; omitted when absent or invalid |

Lists can be empty. Whole order IDs also fit the engine's 512-byte field budget;
16 maximum-length IDs therefore project only the first 15. Opaque or malformed
order IDs are omitted from telemetry under NX's decimal-ID rule, while the
structured error retains its complete native evidence. Unknown indexes come
only from recorded elements or the existing failed index, never inferred from
unissued rows. No URL, endpoint, query, body, signature or arbitrary error text
enters the projection.
