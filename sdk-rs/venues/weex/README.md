# WEEX write uncertainty

The engine seals dispatched HTTP 429, 408 and 5xx writes as unknown before
native authentication/refusal codes can override the outcome. Native code
`429` inside HTTP 200 is also unknown, in both envelopes and per-order rows.
These rate failures direct the caller to Check Open Orders while retaining
native diagnostics; they never authorize a resend or protective put-back.
Reads and documented refusals on non-ambiguous statuses keep their existing
classification.

Venue-decoded Unknown replies report `The outcome is unknown. Check Open Orders`
in the terminal failure and correlated command diagnostic, and emit one
`order.outcome_unknown` event. The indexed journal retains accepted IDs, the
uncertain elements and the unsent suffix. It observes the existing dispatch
policy; payload sizes and write order still follow the legacy TypeScript.

Orders without a caller client ID send one rounded native payload even above
the market maximum, matching legacy `weex.exchange.ts:1041,1112`; the venue's
refusal is surfaced without splitting or retrying. Caller IDs retain the TS
base class's whole-batch maximum-size preflight (`base.ts:1211-1223`).
