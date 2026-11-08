# Coinbase

`spotOnly` selects Coinbase Spot; the default private account discovers its
portfolio type and can select INTX, CFM or Spot. Public-only non-spot accounts
read the drb USDC perpetual catalogue and public stream.

DRB candle history bounds implicit and future-clamped ends with synchronized
protocol UTC, rounded down to whole milliseconds. Scheduling time can stop
during suspend and remains reserved for deadlines and pacing. Explicit range

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

DRB candle history rounds an implicit or host-clamped end time down to whole
milliseconds, since the SDK scheduling clock can be fractional. Explicit range
inputs still require whole milliseconds. A valid summary or stream ticker can
restore a catalogue market omitted or malformed in the initial ticker seed;
other venues retain the shared store's seed policy.

DRB auth invalidation fences both accepted and issued authentication starts.
Replies from an exchange begun before invalidation cannot restore its token.
Fresh exchanges need a strictly later start timestamp, including when the host
clock has not advanced. JWT creation records issued starts without changing the
public auth API signatures; cached tokens retain their zeroizing lifetime.

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

## Provider JWT custody

Registry/SDK construction uses `Coinbase::with_signing`; only public JWT bindings
and opaque references reach the venue. Private REST and both private subscription
frames sign at dispatch. Raw `Coinbase::new` remains the local compatibility API.
Hosted RemoteSigner sends canonical `coinbase-jws` bytes; PayloadSigner is REST
only and cannot authenticate a private socket. Pre-dispatch signing refusal
uses `trading_unsent`, preserving NotSent rather than transport-loss Unknown.
Definite NotSent releases only the reservation acquired by that attempt; it
must neither clear nor restart reconciliation for an older Unknown placement.
The caller contract, algorithm selection and S2/S3 coordination boundary are in
`docs/plans/callers/coinbase-hosted-signing.md` at the root.
