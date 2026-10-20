# Rust port quality standards

Read-only Opus audits of `feat/rust-rewrite` (2026-10-20) for patterns we can
obviously improve without regression. The client contract outside safe-cex may
change where argued; each item names the tealstreet-next callers it touches.
Strike items as PRs merge. Trading-semantic changes are marked **risky**.

## Bugs found (fix now, ahead of the standards)

- Command rejections on Rust venues never emit `'error'`
  (`rust.exchange.ts:857-868`), so the app shows no toast unless the venue also
  logs. Emit once per rejection, de-duplicated against a same-frame `Log Error`.
- Lost-send recovery never runs for Rust: `isUnknownOutcome`
  (`utils/unknownSendOutcome.ts:3`) does not match the Rust "outcome unknown"
  texts (lighter, coinbase, txflow, orderly, bybit). **Risky** (enables lookups
  and chaser decisions).
- A bad key can hang `start()`: Bybit retries startup forever
  (`bybit/src/lib.rs:734-745`) and `untilLoaded` has no failure path
  (`rust.exchange.ts:795`). Needs a terminal startup failure.
- Secrets in logs: 53 log sites interpolate raw bodies; Bitrue logs whole
  private frames (`bitrue/src/account.rs:700`), Bybit logs the signed request
  body (`bybit/src/lib.rs:550`); native transport errors include signed URLs
  (`hosts/native/src/transport.rs:390,404`).
- Native and Durable Object hosts drop logs with no client attached; an
  oversized native broadcast disconnects every peer (`server.rs:386`).

## Error handling

Today: every engine error is a bare string; venue codes are discarded; ~40
hand-written formats; ~95 parse failures collapse to `malformed = true`;
partial-batch detail is JSON inside the error string (`host.mjs:306-317`);
consumers reclassify with regexes (`health/errorPatterns.ts`, app toasts,
`ChaserOrderManager.ts:550`).

Standard (`engine/src/error.rs`):

```rust
pub struct VenueError {
  pub kind: ErrorKind,  // Rejected | InvalidInput | AuthInvalid | AuthExpired | AuthPermission
                        // | RateLimited | Network | Timeout | VenueUnavailable | Unsupported | Internal
  pub outcome: Outcome, // NotSent | Rejected | Unknown
  pub venue_code: Option<String>, pub venue_message: Option<String>,
  pub message: String,  // user-facing; contains venue_message verbatim
  pub retry_after_ms: Option<f64>,
  pub partial: Option<Partial>, // placed ids, failed index, total
}
```

`CommandDone` carries it, `Deactivate{error}` and a terminal
`StartupFailed{error}` replace the reasonless deactivate and the BreakoutProp
prefix match. The wire carries the object. TS exposes `ExchangeError extends
Error` with `kind`, `category` (1:1 onto `ErrorCategory`), `outcome`,
`venueCode`, `venueMessage`, `retryAfterMs`, top-level `partialOrderIds`,
`failedOrderIndex`, redacted `debug`. Regex classification stays only as the
TS-venue fallback.

PRs: (1) types and plumbing, byte-identical messages, URL redaction
(Rust side landed: `engine/src/error.rs`, `Cx::fail`, host `detail` beside
the unchanged `error` text, partial batches as fields; TS `ExchangeError`
follows the bridge PR); (2) one
shared parser per venue family, replacing `malformed` flags; (3) TS toast on
rejection; (4) `outcome: Unknown` at unknown-outcome sites (**risky**); (5)
`StartupFailed`, per-venue auth codes (**risky**: ends infinite retries); (6)
app and chaser read `kind`/`outcome`; (7) wasm panic disposes the engine and
surfaces `Internal`.

tealstreet-next: `exchangeAccountController.ts`, `createLimitOrder.ts`,
`createMarketOrder.ts`, `createScaledOrder.ts` (reads `partialOrderIds`),
`errorToast.tsx`, GRVT builder handler, CLI and exchange-mcp printing.

## Logging and observability

Today: one path, `cx.log(level, String)`, ~460 venue calls (Error 286, Warning
169, Info 4), no Debug, no context, no counters, no redaction. Retried
transients log at Error (toast, Sentry, `console.trace`); some child-leg
failures log at Warning (no toast). `ExchangeHealthMonitor` and the app match
message wording, which makes Rust message text an unversioned API.

Standard: `Notice::Event { level: Debug|Info|Warn|Error, code: &'static str,
message, fields }` with stable dotted codes (`ws.reconnect`, `auth.rejected`,
`snapshot.discarded`, `order.rejected`, `rate_limited`, `child_leg.failed`), a
`Redacted` field type and a 512-byte cap in `Cx`, never raw bodies. Host adds
venue and account. Hand-rolled JSON, no `tracing` crate (wasm size). Host-set
minimum level so Debug costs nothing. Engine `Health` counters (ws opens and
closes, reconnects, auth failures, HTTP failures, rate limited, snapshots
discarded, rows skipped, last private message) exposed via `getHealth()`.
Hosts forward events and write Warn+ to stderr/console with no client.

PRs: (1) facade, wire, `Health`, `cx.log` shim, host events on ws close and
HTTP failure, no-client fallback; (2) redaction sweep with a test that fails on
secrets in any notice; (3) engine-level ws and auth counting; (4) venue batches
assigning codes and fixing levels (keep message text stable); (5) health monitor
and tealstreet-next on codes; (6) delete the shim.

tealstreet-next: `exchangeAccountController.ts:180-383`,
`apps/cli/src/exchange-manager.ts:203`, `apps/cli/src/error-classifier.ts`,
`safeCexTelemetrySink.ts`, `exchange-pretty-error-messages` bybit and bingx.

## Shared engine helpers

Low risk, do first:
1. `engine::de` numeric decoders: 77 hand-written helpers, copies already
   diverge (Binance malformed to 0, Hyperliquid trims, Kraken/WEEX/Coinbase
   reject blanks). Named strict and lenient variants; pin current behaviour
   per venue, then migrate in batches.
2. `engine::url`: one RFC 3986 encoder copied in 7+ venues, plus
   `encodeURIComponent` and axios dialects; 5 `Query` types. Signing vectors
   settle it.
3. `engine::backoff::Backoff`: ~40 inline exponential formulas, no jitter.
   Migrate with constants unchanged, then jitter, then one startup-retry policy
   (fixes startup fail-closed on OKX, BloFin, Bitget).
4. `engine::time`: ISO/civil-date math in 9 venues.

Medium:
5. `SnapshotBuilder<T>` with `push`, `reject(id, err)`, `commit`: a rejected
   row keeps the store's copy. Structurally ends the one-bad-row-discards-the-
   snapshot class (~50 hand-rolled `invalid` flags).
6. Per-venue `from_options` in a `safe-cex-venues` crate shared by hosts;
   delete ~130 lines of TS credential checks (`rust.exchange.ts:431-645`);
   route `tickInterval` to every venue.
7. Typed `Interval` parsed once; venues reject unsupported intervals instead of
   silently falling back (Hyperliquid 1m, TxFlow 30d, AsterDEX 1h).

Bridge (contract change):
8. Engine-reported capabilities replace 76 venue-name branches in
   `rust.exchange.ts` and `FrameApplier` positional booleans. tealstreet-next:
   `collectAccountAxes.ts`, `ClosedPositions.tsx`, `symbolScopedFills.ts`.
   Keep `contractsNative`/`remainingNative` names (**risky** if their
   computation moves).

Risky, after live-smoke captures:
9. `num::whole` for leverage (five serialisations today, incl. truncation and
   `20.0`) and one Rust owner for client-id rules.

Lower: shared test-support `ProbeVenue` (12 copied `secondary_tests.rs`),
`RangeWalk` for the 16 hand-rolled history paginators, one `SocketSpec`.
