# Shared public startup retry reporting

Scope: RUST_QUALITY shared helper 3 tail, OKX/BloFin/Bitget only.

Re-verified on landing a07b6b35c87: all three already retry public discovery
through engine::startup::Backoff. The shared exponential schedule, jitter and
socket Supervisor are landed; Bybit #1608 extends existing cooldown timers.
The remaining work is visible, recoverable startup degradation. OKX also
cancels its retry after an aggregate empty catalogue that Store refuses.

Overseer decision: never stop retrying after a bound. After six failures of
one required public read, report startup.degraded once with the latest typed
cause. Keep the capped backoff running; clear the report on successful public
startup. Leave private reads, Bybit and post-loaded recovery unchanged.

1. Extend the existing startup helper with one public failure/reporting path
   used directly by all three venues. Reuse failed_after's schedule, timer
   deduplication and cooldown rules. Emit coded Warn startup.degraded with
   typed kind, read, attempts and safe venue cause fields, and Info
   startup.recovered after markets and tickers become usable. Warn avoids
   treating a recoverable public outage as an SDK credential-validation error.
   Keep the default helper's private/other-venue behavior unchanged.
2. Route public transport, envelope and structural failures through that path;
   keep typed venue code/message when available without logging raw bodies.
   Fix aggregate empty OKX catalogues without rejecting legitimately empty
   product scopes alongside valid peers. Retain valid-row/partial-catalogue
   behavior, signed request handling and post-loaded retry behavior.
3. Prove threshold, continuing capped retries, duplicate/ban behavior,
   independent reads, safe latest cause and recovery with engine and per-venue
   regressions. Record baseline failures before implementation, preserve all
   existing tests, and run touched venue plus Binance tests, scoped strict
   Clippy/fmt, independent audits and full CI. Merge latest related landing,
   run a fresh separate revert guard, then merge green and report via Aimux.

The coded events are recoverable reporting receipts and helper state; this
change does not introduce persistent health-badge semantics or a terminal
StartupFailed notice. Existing hosts already forward event codes and fields.
