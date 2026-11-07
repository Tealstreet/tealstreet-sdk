# Coinbase

`spotOnly` selects Coinbase Spot; the default private account discovers its
portfolio type and can select INTX, CFM or Spot. Public-only non-spot accounts
read the drb USDC perpetual catalogue and public stream.

## Advanced Trade ambiguous writes

Released Advanced Trade creates, edits and cancellations answered with HTTP
408, 429 or 5xx remain `Unknown`, including ordinary and protective orders.
Native `RESOURCE_EXHAUSTED` / `rate_limit_exceeded` codes also remain unknown;
only native code fields count, not message substrings. Status uncertainty takes
precedence over native refusal text. Preserve confirmed siblings in batch cancels.

Unknown writes say "Check Open Orders", retain placement reconciliation state
and never resend automatically. Genuine documented refusals remain `Rejected`.
Account settings and protective amendments are unsupported before dispatch and
remain `NotSent`; they have no HTTP failure matrix.

Native codes: [Coinbase error messages](https://docs.cdp.coinbase.com/coinbase-app/api-architecture/error-messages).
Fixtures: `tests/trading.rs` (`advanced_*`); no live credentials are needed.

## INTX startup migration message

When a positively identified INTX bootstrap read is unavailable (HTTP 404, 408,
410 or 5xx, or a transport failure/timeout), emit one error and terminal
`StartupFailed` with kind `unsupported` and this exact text:

> Coinbase International moved to Coinbase's new derivatives API; an update is coming.

This covers initial International instruments, an INTX portfolio probe,
INTX product discovery and initial INTX balance/orders/positions reconciliation.
Streamed JSON errors and HTML gateway responses follow the same path. A failure
before the portfolio type is known does not establish INTX. Authentication
errors, rate limits, malformed successful responses, Spot/CFM and refreshes
after startup keep their existing handling. A complete, usable International
instrument array remains accepted even with an error HTTP status. Deferred
catalogue commands still settle through the existing read-failure path. The
migration message does not deactivate credentials. Public drb discovery and
refresh failures retain their bounded retry handling and never emit this legacy
INTX migration message.

Deterministic coverage: `tests/intx_moved.rs` and `tests/startup_resilience.rs`.
No live credentials or private requests are needed.
