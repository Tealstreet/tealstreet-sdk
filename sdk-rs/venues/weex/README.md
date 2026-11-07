# WEEX write uncertainty

The engine seals dispatched HTTP 429, 408 and 5xx writes as unknown before
native authentication/refusal codes can override the outcome. Native code
`429` inside HTTP 200 is also unknown, in both envelopes and per-order rows.
These rate failures direct the caller to Check Open Orders while retaining
native diagnostics; they never authorize a resend or protective put-back.
Reads and documented refusals on non-ambiguous statuses keep their existing
classification.
