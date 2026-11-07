# Coinbase

`spotOnly` selects Coinbase Spot; the default private account discovers its
portfolio type and can select INTX, CFM or Spot. Public-only non-spot accounts
read the drb USDC perpetual catalogue and public stream.

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

Deterministic legacy coverage: `tests/support/legacy_intx_moved.rs` (unit-only
legacy protocol selector) and `tests/startup_resilience.rs`. Authenticated drb
startup is covered by `tests/support/private_drb.rs`; after authentication it
uses S1's public drb catalogue and native `{BASE}_USDC-PERPETUAL` symbols.
No live credentials or private requests are needed.
