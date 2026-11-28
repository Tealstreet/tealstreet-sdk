# Unknown-outcome telemetry

Bitunix registers the static engine hook for engine-sealed Unknown completions,
including HTTP 408/429/5xx and lost replies in ordinary and frame-fed modes.
The callback reads sealed facts and returns a warning event. It has no request,
store, classification or retry authority. Native-decoded 2xx Unknowns use the
same projection through the venue's existing completion path. The engine hook
does not run for those native results.

Native failure reporting follows the classified write outcome. Code `30042`
remains Unknown and emits once even though its raw decoder fallback says
Rejected; that fallback does not decide whether telemetry is published.

`order.outcome_unknown` uses the canonical Unknown guidance as its message and
only the fields admitted by NX `engineEventTelemetry` FIELD_RULES (#9262):

| Field | Projection |
| --- | --- |
| `command` | `bitunix.<decimal command ID>`, matching `[a-zA-Z][a-zA-Z0-9_.-]{0,63}` |
| `request` | Internal decimal request ID, matching `[0-9]{1,20}` |
| `outcome` | Literal `unknown` |
| `placedOrderIds` | Up to 16 comma-separated native IDs, each matching `[0-9]{1,32}` |
| `unknownIndices` | Up to 16 comma-separated decimal indices in 0–65535 |
| `status` | HTTP integer 100–599; omitted without a valid status |

Empty lists are empty strings. Invalid identities or indices are omitted, and
list entries retain their original order. Lists also fit the engine's 512-byte
field cap without truncating an identity: at most 15 maximum-width native IDs
fit that byte bound. No raw error text, client IDs, URL, endpoint, query string,
body, signed payload, credential, timestamp or extra field enters the projection.

The request correlation is retained on the existing live command solely for
native telemetry. It does not change request construction, signing, dispatch,
continuation or retry decisions. Regression tests cover ordinary/frame-fed
HTTP and transport failures, native Unknowns, definite refusals, late replies,
closed field schemas, unsafe source text and the NX bounds and list caps.
