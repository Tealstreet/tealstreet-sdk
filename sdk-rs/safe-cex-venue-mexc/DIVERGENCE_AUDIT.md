# MEXC divergent audit

The user stopped this unit before venue closure. This is a repair handoff and
bounded audit ledger, not a completed-venue claim. No push was made.

Integration parent: `0286f63437e4320e8c12886d5fef8fc6dfa2eac3`, merged normally
into `fix/mexc-divergent-closure`. Earlier local repair commit `633034194e`
is part of this same deliverable, not a separately accepted dependency.

The frozen TOML population is 145 ported / 40 divergent / 0 pending, 185 rows.
Status moves are zero. The inventory below is independently enumerated in
manifest filename/source-row order and stops at iteration 40, removeHandler.
The 40 residual rulings are disjoint: 21 CHOSEN, 17 MIXED, 0 FORCED,
0 CORRECTED, 2 FINDING. These are annotation outcomes; they do not imply
all-input behavioral conformance or completion of the two class findings.

The canonical criteria remain at
`sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy`,
`#shared-utf8-string-boundary`, `#closed-typed-output-boundary`,
`#owned-snapshot-vs-live-alias`, `#numeric-ingress-defect`, and
`#forced-surrogate-conflict`. Local decisions cite the explicit MEXC anchors in
that specification; this ledger does not redeclare them.

| # | Source symbol | Residual | Primary outcome in this unit |
|---|---|---|---|
| 1 | serializeMexcParams | CHOSEN | Supported existing input/collation ruling |
| 2 | normalizeRequestData | MIXED | Explanation: actual returned config.data units |
| 3 | getQueryString | CHOSEN | Supported existing input/collation ruling |
| 4 | createAPI | MIXED | Supported existing factory-specific floors |
| 5 | extractRows | MIXED | Supported existing borrowed/prototype/unit floor |
| 6 | MexcExchange.validateAccount | CHOSEN | Behavior repair: response message/msg selection |
| 7 | MexcExchange.getAccount | CHOSEN | Behavior repair: delegated detail and coercion name |
| 8 | MexcExchange.fetchMarkets | CHOSEN | Behavior repair: raw object reporting |
| 9 | MexcExchange.fetchTickers | CHOSEN | Behavior repair: raw object reporting |
| 10 | MexcExchange.fetchBalance | CHOSEN | Explanation: inherited currency-read cost |
| 11 | MexcExchange.fetchPositionMode | CHOSEN | Supported existing error-origin floor |
| 12 | MexcExchange.fetchPositions | CHOSEN | Behavior repair: raw object reporting |
| 13 | MexcExchange.fetchAndSetOrders | CHOSEN | Scheduling explanation and native prefix floor |
| 14 | MexcExchange.fetchAndSetRegularOrders | CHOSEN | Scheduling explanation and native prefix floor |
| 15 | MexcExchange.fetchHistoricalFills | MIXED | Supported existing historical-query floor |
| 16 | MexcExchange.fetchOHLCV | CHOSEN | Behavior repair: original transport provenance |
| 17 | MexcExchange.cancelOrders | CHOSEN | Supported existing cancellation-error floor |
| 18 | MexcExchange.updateSymbolMetadata | CHOSEN | Supported existing Store-throw floor |
| 19 | normalizeMarket | MIXED | Supported existing units/owned-info witnesses |
| 20 | normalizeTicker | MIXED | Supported existing units/owned-info witnesses |
| 21 | normalizeBalance | CHOSEN | Explanation: homogeneous own-currency map |
| 22 | normalizePosition | MIXED | Supported existing units/owned-info witnesses |
| 23 | normalizeOrder | MIXED | Supported existing units/owned-info witnesses |
| 24 | normalizePlanOrder | MIXED | Supported existing units/nested-alias witnesses |
| 25 | normalizeStopOrder | MIXED | Supported existing units/nested-alias witnesses |
| 26 | normalizeExecution | MIXED | Supported existing units/typed-asset refusal floor |
| 27 | MexcPrivateWebsocket | FINDING | Construction repaired; remaining composition proof open |
| 28 | MexcPrivateWebsocket#handleOrder | MIXED | Supported existing committed Store/units/error floor |
| 29 | MexcPrivateWebsocket#handleFill | MIXED | Supported existing typed native fill floor |
| 30 | MexcPrivateWebsocket#handlePosition | MIXED | Supported existing mode/Store/units/error floor |
| 31 | MexcPrivateWebsocket#handlePlanOrder | MIXED | Supported existing branch/Store/units/error floor |
| 32 | MexcPrivateWebsocket#handleStopOrder | MIXED | Supported existing branch/Store/units/error floor |
| 33 | MexcPrivateWebsocket#applyStopLikeFrame | MIXED | Supported existing direct-branch floor |
| 34 | MexcPublicWebsocket | FINDING | Construction repaired; remaining composition proof open |
| 35 | MexcPublicWebsocket.constructor | CHOSEN | Behavior repair: concrete parent/listener binding |
| 36 | MexcPublicWebsocket.onMessage | CHOSEN | Coverage: registered ticker route commits before throw |
| 37 | MexcPublicWebsocket#subscribe | CHOSEN | Behavior repair: actual registered caller/state |
| 38 | MexcPublicWebsocket#unsubscribe | CHOSEN | Behavior repair: actual registered caller/state |
| 39 | MexcPublicWebsocket#addHandler | CHOSEN | Behavior repair: inherited non-array rejection |
| 40 | MexcPublicWebsocket#removeHandler | CHOSEN | Behavior repair: inherited non-array rejection |

Primary outcomes are also disjoint: 21 supported existing rulings,
3 explanation repairs, 11 behavior-repair rows, 2 scheduling-explanation rows,
1 coverage repair, 2 open findings. Multiple rows can share one implementation
repair, so eleven is not a count of independent code defects.

## Repairs and limits

Account validation now retains response.data.message before msg, the transport
message and the empty-error fallback. The selected raw JSON remains in the
validation result. getAccount preserves a coercion TypeError. Raw snapshot
reporting selects the enhanced-message branch before attempting ordinary
message coercion, and reports JSON object detail without the old Message-path
prefix. The separate callback/error projection ruling is
`MANIFEST-SPEC.md#mexc-report-input-choice`.

OHLCV now preserves the rejected RestFailure origin, response and original
request/message. Its new rejection witness is native only; the maintained
source's uncaught request is inspected, not a new source rejection execution.
The historical runtime constructor-interval observation was already withdrawn
as outside the maintained declared Timeframe input; it is not counted as an
admitted-input defect repaired here.

The concrete factory registers both socket owners on one exchange and Store.
Public heartbeat/reconciliation, candle and book listeners, cleanup, private
fill dispatch and local login now have actual native bindings. Subscription
iteration releases its borrows before invoking send/diagnostics. The shared
protocol disposal hook reaches the venue heartbeat and listener cleanup.
Representation and host costs cite
`MANIFEST-SPEC.md#mexc-native-socket-owner-choice`.

The private class finding remains open for timer-driven Ready balance-reaction
and remote-signing-prefix controls. Its balance continuation now uses the
reaction helper and checks the disposed socket, but the new timer path was not
independently discriminated. The public class finding remains open for
send-time reentrant resubscription mutation controls and complete facade/source
callback composition proof. Passing initial-open and listener fixtures does
not establish those missing proofs.

Currency prototype reads are locally ruled at
`MANIFEST-SPEC.md#mexc-currency-own-record-choice`; source inherited free7 and
prototype total9 remain unavailable, while own constructor.free2 survives.
Info alias observations retain their separate section16 ruling. The four-method
native unpolled/drop/polled request inventory is priced at
`MANIFEST-SPEC.md#mexc-borrowed-method-choice`.

## Evidence boundary

Scoped native/source tests cover the touched account/reporting/provenance,
registered socket, callback-array and currency-projection paths. Existing
mapper Unicode, historical-query, cancellation and private-handler corpora were
inspected for this ledger; their old results are not re-reported as fresh runs.
The delivery receipt records the measured scoped test exits and gate exits.

One deliberate fail was observed before the account fix: the old implementation
failed `MEXC validation selects response detail before transport message` with
transport failure versus response detail. This ran against the old production
body plus the changed assertion; it is not a gate on the delivered SHA.
A later restored run supplies the passing half. Three initial test failures were
fixture mistakes: the exchange uses BTCUSDT, the URL fixture supplied a slash,
and a fake time1250 suppressed the centrally debounced reconnect event. They
were corrected against the maintained source/core behavior and are not counted
as mutation controls. Compilation errors during authoring are also not controls.
No broader new-family mutation suite or vacuity-free full class proof was run;
the two retained findings make that incomplete work explicit.

Generated logs and SHA-bound receipts stay ignored under
`sdk-rs/target/mexc-divergence-audit/`. The final receipt contains the exact
execution-input SHA for cargo fmt --check and cargo build --workspace --locked.
Clippy, wasm, wiring, layers and routing are centrally owned and were not run.
