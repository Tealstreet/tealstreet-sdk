# BingX read refusals retain rate-limit evidence

BingX may return HTTP 200 with `code: 100410` and no list. Its official
[error-code reference](https://github.com/BingX-API/api-ai-skills/blob/main/skills/references/error-codes.md)
defines any nonzero envelope code as failure and 100410 as a rate limit.
List absence in such an answer is not a malformed snapshot.

Rust now classifies that envelope and HTTP 429 through the existing IP-ban
policy before projecting list-decoding failures. Discovery (contracts, daily
tickers and premium index), account balance/positions/open orders, candles,
identity's balance list and all history lists retain `kind: rateLimited`.
Caller-correlated position, candle, identity and metadata reads retain the
typed error; background snapshots retain it in structured failure notices.
Leverage and margin metadata use the same classification.

`retryAfterMs` comes from Retry-After or the message's unblock epoch, using
the existing five-minute fallback and 24-hour validation ceiling. The
response's own server-clock timestamp is not an unblock time. Public discovery
retains its existing ban-aware retry schedule; this change adds no new retry
or write policy. Native code/message remain available when present.

SDK, NX and server callers should use the typed kind and delay instead of
treating the refusal as corrupt data or a credential error. Failed snapshots
retain the previous financial state. Auth refusal, genuine malformed-list
diagnostics and successful empty/null lists keep their existing behavior.
No caller API or deletion prerequisite changes.
