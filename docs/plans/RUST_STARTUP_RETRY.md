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

Validation: 13 new tests (10 venue regressions and three engine policy tests).
The initial three missing-degradation cases and empty OKX catalogue reproduced
on landing; review's three malformed-preview cases reproduced before the fix.
All 1,106 engine/OKX/BloFin/Bitget/Binance tests pass. The new typed
on_stream_malformed callback preserves the default behavior for other venues
and private reads while keeping raw previews out of public degradation.
Independent final review found no blocker and retained every existing test and
assertion. Capped retries, rate-limit minimums, duplicate timer suppression,
independent read counters, latest venue cause and recovery are covered.
