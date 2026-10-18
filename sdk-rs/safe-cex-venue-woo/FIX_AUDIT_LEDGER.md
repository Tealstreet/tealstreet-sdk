# WOO fixed-cohort audit ledger

This ledger tracks the exact 60 assigned non-UTF-8 FIX-audit identities. It does
not replace the older cumulative DIVERGENCE_AUDIT_LEDGER.md or claim that all
historical choices are authorized. OPEN rows require the listed follow-up.

The constructed native exchange probe checks all 47 V3 names against the actual
receiver and runtime globals, with positive checks on the installed legacy methods
and both sockets. The maintained whole-class source probe constructs two receivers,
walks their legacy module dependencies and checks all eight hard-false gates.
These show no production registration; they do not prove standalone V3 algorithms.

For the recovery island, repository caller searches find only its own module and
facade test on Rust, and only audit tests on TypeScript. The unused Rust module,
its facade test, oracle, controls, census and obsolete gate receipts are removed.
The compiler and the actual legacy snapshot/delta/cleanup test verify the deletion.
The 23 source manifest rows remain present as not-applicable with that evidence.
No production TypeScript or shared type boundary changes.

The cancel-all-after island likewise has no production importer or caller. Its
Rust descriptor/response module and facade-only tests are removed, including
unused cancel-schema cases in the position-mode oracle. The maintained WOO class
inherits `BaseExchange.scheduleCancel` at `packages/safe-cex/src/exchanges/base.ts:989–991`:
`Error` / `Not implemented`, for undefined, null, zero and NaN. The actual native
binding agrees, emits no additional report, and creates no HTTP or socket route.
The signed legacy `cancelAllOrders` DELETE remains operational.

The V3 request preparation functions and auth/prepared DTOs have no production
TS or Rust consumer. The constructed class dependency walk never loads
woo.v3.request.ts. Their unused Rust bodies and exact facade cases are retired;
WooV3RequestInput stays for the remaining inert builders. The full ticker/quote
corpus, including native bits and alias/freeze observations, stays unchanged.

The strict V3 account-info decoder and account-balance mapper have no production
TS or Rust caller. Their DTO, private enum/helper and exact facade tail are removed.
The remaining asset/futures/helper/pagination corpus is preserved, while the real
legacy validateAccount/fetchBalance signed HTTP path still runs. The similarly named
v3_contracts::WooV3AccountInfo declaration is separate and stays unchanged.

## Exact assignment

| ID | Source identity | Outcome | Native evidence / remaining work |
| --- | --- | --- | --- |
| 1 | `WooV3PreparedRequest` | ACCEPT | No production caller/registration; unused preparer, auth/prepared DTOs and exact facade tail retired. Request input and every unrelated quote/ticker case remain; native/source registration probes and all-target compiler verify the boundary. |
| 2 | `prepareWooV3Request` | ACCEPT | No production caller/registration; unused preparer, auth/prepared DTOs and exact facade tail retired. Request input and every unrelated quote/ticker case remain; native/source registration probes and all-target compiler verify the boundary. |
| 3 | `ORDER_TYPE` | OPEN | Existing live native witnesses available; reconcile this exact lead and refresh heartbeat/mapper/ticker receipt before ACCEPT. |
| 4 | `ORDER_SIDE` | OPEN | Existing live native witnesses available; reconcile this exact lead and refresh heartbeat/mapper/ticker receipt before ACCEPT. |
| 5 | `ORDER_TRIGGER_TYPE` | OPEN | Existing live native witnesses available; reconcile this exact lead and refresh heartbeat/mapper/ticker receipt before ACCEPT. |
| 6 | `validateTarget` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 7 | `decimalToNumber` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 8 | `mapWooV3AccountBalance` | ACCEPT | No production caller/registration; strict account decoder, balance mapper, DTOs and orphan enum/helper/facade tail retired. Native/source boundary probes and actual legacy balance HTTP plus all-target compiler verify the removal. |
| 9 | `WooV3CancelAllAfterResponse` | ACCEPT | No production caller/registration; descriptor/response module and exact facade tail retired; native/source inherited scheduleCancel rejection agrees and signed cancelAllOrders still works. |
| 10 | `WooV3OrderBookRecoveryCoordinator.pushUpdate` | ACCEPT | No production caller or runtime registration; isolated recovery port and facade tail retired; real legacy orderbook probe retained. |
| 11 | `WooPrivateWebscoket.ping` | OPEN | Existing live native witnesses available; reconcile this exact lead and refresh heartbeat/mapper/ticker receipt before ACCEPT. |
| 12 | `mapWooV3Candles` | ACCEPT | Only TS audit/comparison facades and Rust oracle tests call this mapper; the constructed class imports no V3 modules and installs no V3 member. The unused Rust mapper, nested sort, its reviewed local-function row and exact facade/control tail are retired. The independent kline decoder/record remain. Actual signed legacy recent/proxy candle reads and source/native registration probes pass before and after; all-target compiler checks the remaining callers. |
| 13 | `WooV3SetLeverageInput` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 14 | `WOO_V3_ACCOUNT_WRITE_PARITY` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 15 | `requirePerpetualSymbol` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 16 | `positionProvesNoExposure` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 17 | `WooPublicWebsocket.ping` | OPEN | Existing live native witnesses available; reconcile this exact lead and refresh heartbeat/mapper/ticker receipt before ACCEPT. |
| 18 | `WooPublicWebsocket.handlePingEvent` | OPEN | Existing live native witnesses available; reconcile this exact lead and refresh heartbeat/mapper/ticker receipt before ACCEPT. |
| 19 | `WOOXExchange.fetchHistoricalFills` | OPEN | History page admission repaired: actual signed callers now skip null/array rows without symbols and report original primitive lookup failures; 14 source/native profiles inspect all output keys and numeric bits. Native signed NaN query repaired in #1060; retained-response reporting in merged #1089 preserves non-strings and lone surrogates across 26 signed HTTP profiles; other parser/output choices still need separate review. |
| 20 | `WOOXExchange#fetchHistoricalTradesInternal` | OPEN | History page admission repaired: actual signed callers now skip null/array rows without symbols and report original primitive lookup failures; 14 source/native profiles inspect all output keys and numeric bits. Native signed NaN query repaired in #1060; retained-response reporting in merged #1089 preserves non-strings and lone surrogates across 26 signed HTTP profiles; other parser/output choices still need separate review. |
| 21 | `WOOXExchange.mapLimitOrder` | OPEN | Existing live native witnesses available; reconcile this exact lead and refresh heartbeat/mapper/ticker receipt before ACCEPT. |
| 22 | `WOO_V3_WS_CHANNELS` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 23 | `WOO_V3_WS_LIMITS` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 24 | `WOO_V3_WS_PUBLIC_TOPIC_KINDS` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 25 | `WOO_V3_WS_PRIVATE_TOPICS` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 26 | `WOO_V3_WS_BLOCKERS` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 27 | `WooV3ListenKey` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 28 | `buildWooV3PublicTopic` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 29 | `buildWooV3PrivateTopic` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 30 | `parseWooV3WsTopic` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 31 | `WOO_V3_WS_COMMAND_ID_CONTRACT_STATUS` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 32 | `WooV3WsCommandAck` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 33 | `WooV3WsPing` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 34 | `WooV3WsPong` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 35 | `WooV3WsOrderBookUpdate` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 36 | `WooV3WsTicker` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 37 | `WooV3WsKline` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 38 | `WooV3WsPrice` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 39 | `WooV3WsFundingRate` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 40 | `WooV3WsOpenInterest` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 41 | `WooV3WsPublicPayload` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 42 | `WooV3WsAccount` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 43 | `WooV3WsBalanceRow` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 44 | `WooV3WsExecutionReport` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 45 | `decodePrivatePayload` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 46 | `advanceWooV3OrderBookSequence` | ACCEPT | No production TS caller or native runtime registration; isolated Rust helper, result enum, orphan codecs and exact facade/oracle/control tail retired. The maintained constructed-class/native registration probes and actual legacy listenOrderBook snapshot/delta/cleanup target pass before and after; all-target Clippy compiles the remaining consumers. See WS_STATE_GATES.md. |
| 47 | `WooV3AlgoOrderMapping` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 48 | `mapRegularOrderType` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 49 | `WooPublicWebsocket.handleTickersStreamEvents` | OPEN | Existing live native witnesses available; reconcile this exact lead and refresh heartbeat/mapper/ticker receipt before ACCEPT. |
| 50 | `WooPublicWebsocket.handleBBOStreamEvents` | OPEN | Existing live native witnesses available; reconcile this exact lead and refresh heartbeat/mapper/ticker receipt before ACCEPT. |
| 51 | `WooPublicWebsocket.handleMarkPricesStreamEvents` | OPEN | Existing live native witnesses available; reconcile this exact lead and refresh heartbeat/mapper/ticker receipt before ACCEPT. |
| 52 | `WooV3AccountInfo` | ACCEPT | No production caller/registration; strict account decoder, balance mapper, DTOs and orphan enum/helper/facade tail retired. Native/source boundary probes and actual legacy balance HTTP plus all-target compiler verify the removal. |
| 53 | `decodeMeta` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 54 | `WooV3WsBlocker` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 55 | `WooV3AlgoOrder` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 56 | `normalizeUpdate` | ACCEPT | No production caller or runtime registration; isolated recovery port and facade tail retired; real legacy orderbook probe retained. |
| 57 | `WooV3SetLeverageBuildResult` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 58 | `WooV3AccountMutationResult` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 59 | `WooV3WsPrivatePayload` | OPEN | No runtime registration observed by assigned_v3_rows_have_no_constructed_runtime_registration; inspect caller closure and retire unused Rust port before ACCEPT. |
| 60 | `WOOXExchange` | OPEN | Eager invocation repaired in #1066; full class metadata/cache reconciliation remains open. |

## Scoped proof

- `fix_audit_tests::`: the real runtime-registration and maintained source probes
  pass before and after the recovery module is removed.
- `binding_streams::listen_order_book_joins_the_snapshot_with_buffered_deltas`:
  the actual exchange, configured snapshot client, public socket, buffered delta join,
  native orderbook and cleanup path remain operational.
- All-target compiler checking covers the remaining WOO consumers; the full parity
  index checks that no removed Rust target or obsolete exemption is left behind.

The recovery, cancel-all-after, request-preparer, account-read, candle-mapper and
sequence batches accept 9 of 60 assigned rows; the remaining 51 stay OPEN.
Of those, 38 are other inactive V3 rows whose Rust caller closures still need retirement.
