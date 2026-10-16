# MEXC and AsterDEX divergence explanations, with finding types

Frozen integration parent: `d4e20845a85cc4e2711c99648ef7547f8aa806df`; selection committed before analysis at `735f70c005`. No unreviewed feature ancestor. The requested40/33 figures are floors: actual manifests have **40 MEXC and92 AsterDEX divergent rows**, so all132 are frozen and inspected in source-path/symbol order without substitution. Stops: MEXC40 `MexcPublicWebsocket.onMessage`; AsterDEX92 `AsterDEXPublicWebsocket.subscribeToKline`.

| Venue | Ported/divergent/pending | Supported | Finding | Unresolved | Explanation | Scheduling | Documentation | Previously known | New behavior |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| MEXC |145/40/0|2|31|7|9|4|2|16|0|
| AsterDEX |130/92/9|36|54|2|20|0|15|19|0|
| Full132 |275/132/9|38|85|9|29|4|17|35|0|

**Zero status, target, identity or file-target moves.**53 notes gain historical-preserving annotations:50 new primary findings plus three secondary documentation corrections on already-known rows (MEXC29, AsterDEX47/48).35 previously-known rows are not35 newly discovered independent bugs: they include shared numeric, transport, metadata and lifecycle/composition causes. Zero new behavior findings is not a claim that current native behavior has no defects. A supported explanation does not prove whole-row conformance. No production code or existing native tests change.

Canonical declarations remain at [14](../../MANIFEST-SPEC.md#shared-utf8-string-boundary), [15](../../MANIFEST-SPEC.md#closed-typed-output-boundary), [16](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [17](../../MANIFEST-SPEC.md#numeric-ingress-defect), [18](../../MANIFEST-SPEC.md#divergence-taxonomy), and [19](../../MANIFEST-SPEC.md#forced-surrogate-conflict). Applications below are observations, not duplicate policy definitions.

## Evidence boundaries

At source input `1509a7993843aa4aa4df247fd32bfa80f43bb15e`, all15 MEXC profiles and all4 AsterDEX profiles pass. Each targeted in-memory source control fails at its exact runtime assertion with exit1 and then passes restored: **19 selected profiles, each0/1/0**. See `measured-1509a79938/controls.json` and `aster-measured-1509a79938/controls.json` with hashed logs. The profile stops are `mexc-query-encoding` and `aster-public-message-consumer`; these are not132 independently mutated native bodies. Materialized units, request effects, Store target/value and event fields are asserted before JSON output. No roundtrip equality gate or new Rust/native runtime is claimed.

The MEXC probes establish four source invocation prefixes, seven returned symbols containing D800, actual factory request callback/private-normalizer output, and actual installed response callback output. Actual HMAC and AxiosHeaders run with public synthetic test bytes; no request is sent. Native futures and required String/Value refusal costs are inspected only. Private getQueryString's D800 and FFFD inputs produce equal observed HMACs through the actual hasher; that specific consumer does not prove a public retained-unit output. The qs probe establishes two ASCII-only encoded fields for its two inputs: **no literal lone unit retained in that output**, not universal information erasure or encoder equivalence. The source log's shorthand “erases” is limited to this observation.

The AsterDEX probes execute actual public onOpen/subscribe, actual private onMessage through actual BaseExchange.log into a recording program emitter, actual private fetchListenKey through actual public connectAndSubscribe into a recording socket-URL callback, and actual public onMessage/parser/ticker handler into Store lookup/write plus ticker event. Observed units are respectively D83D at JSON-summary index199, D800 at the socket URL end, and D800 in the event key. These are program observations before serialization. They do not prove live socket/network acceptance, complete SDK composition or native decoding/error outputs.

Initial input `9415fc16f8` is **not** an accepted all15 control batch: fetchTickers' control selector matched both outer and reducer arrows, failing setup rather than the named timing assertion. Its accepted=false receipt and every log remain preserved. `source-control-repair.json` explains the exact call site and later prefix-selector repair. The fresh15-profile measurement at1509 proves the repaired named assertion; no failed control is silently dropped. All19 final selected controls are detected; no survivor is claimed beyond that floor.

## Adjudications and unresolved proof

`review-challenges.json` retains two withdrawn proposals: the MEXC invalid-Timeframe observation does not prove an admitted source caller; AsterDEX's already-declared extra-await cost remains narrowly supported rather than a new scheduling finding. The two formerly unresolved AsterDEX listen-key/public-message unit claims gained actual consumer controls and are now explanation findings, with native outcomes still inspection-only.

Seven MEXC unresolved rows:11 invalid interval input-domain;16 wrapper notification;28/30/31/33 exact private handler notification witnesses;40 actual public message-route notification witness. Two AsterDEX unresolved rows:10 private open-interest helper lacks a public retained-unit consumer observation;14 native eager-start promise depends on host launch/poll contract not established by the inspected witness. A working lower-level callback test does not prove the unexecuted wrapper/route, and absence of a witness is not a newly proved runtime defect. Their historical notes remain intact.

Documentation findings include stale handler-only stories after concrete stream wiring landed, stale pending-mapper claims, a MEXC method whose source already projects errors to text, and four AsterDEX type notes assigning opt-in repairs to the wrong record/field. Available numeric-ingress opt-in does not by itself rewire every production consumer or repair ordinary Serde. The remaining known numeric losses stay separate.

## Checker-artifact screen

No exact public `pub use types::*` plus same-file local target struct suspect was established in the inspected target files. Two **private-glob analogues** are flagged: `account_fetch.rs::AccountFetch` and `order_execution.rs::OrderExecution`. Current checker `reexports.rs` has an unconditional local-nominal bypass; code inspection is not an executed checker result. No row is classified as a proved checker artifact and no semantic rationale finding relies solely on checker rejection. We do not claim global parity-check clearance or absence of every possible indexer bug.

## Per-row rulings

<a id="mexc-1"></a>

### mexc 1. `createAPI` — finding / explanation

Actual installed factory response callback returns the same response containing symbol unit D800; required native response Value/String cannot retain the unit, so whole decoded response is unavailable. Historical blanket CHOSEN does not identify this program observation/cost under section19. Actual HMAC/AxiosHeaders ran with public test bytes; no request sent or native decoder executed.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.api.ts:51". Native: "sdk-rs/safe-cex-venue-mexc/src/client.rs:182". Source profile: `mexc-factory-response`. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict).

<a id="mexc-2"></a>

### mexc 2. `getQueryString` — supported / input-boundary

Narrowly Supported: the explicit owned JSON/BigInt and hook/cycle input exclusions are a representation choice with rationale. The private URL-suffix String flows to the factory HMAC consumer; do not infer a forced retained-unit loss solely from inspecting this private helper's temporary result. No whole collation/input-domain parity proof.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.api.ts:44". Native: "sdk-rs/safe-cex-venue-mexc/src/api.rs:130". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-3"></a>

### mexc 3. `normalizeRequestData` — finding / explanation

Actual private normalizeRequestData flows through the actual factory request callback to returned config.data containing unit D800. Required JsJsonValue::String and RestRequest string body cannot retain that unit; refusal loses the outgoing request value. Historical blanket CHOSEN does not identify this program observation/cost under section19. Actual HMAC/AxiosHeaders ran with public test bytes; no request sent or native decoder executed.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.api.ts:39". Native: "sdk-rs/safe-cex-venue-mexc/src/api.rs:41". Source profile: `mexc-normalize-request-consumer`. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict).

<a id="mexc-4"></a>

### mexc 4. `serializeMexcParams` — supported / input-boundary

Narrowly Supported: the note explicitly prices owned JSON/BigInt versus runtime hooks, Date and cycles. qs encoding returns an encoded query String; no exact materialized-unit retention has been established by this review. Default locale/collation is outside this narrow support and was not executed.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.api.ts:17". Native: "sdk-rs/safe-cex-venue-mexc/src/api.rs:65". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#closed-typed-output-boundary).

<a id="mexc-5"></a>

### mexc 5. `MexcExchange.cancelOrders` — finding / existing

Known concrete open transport gap: TS cancelOrders has no transport catch and rethrows the original rejection; native post_cancel_payload replaces it with failure_message and a new ExchangeError, dropping response/code. Existing source/native assertions at exchange_tests.rs3061 explicitly distinguish both views. Static inspection only.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts:1170". Native: "sdk-rs/safe-cex-venue-mexc/src/exchange.rs:305". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-6"></a>

### mexc 6. `MexcExchange.fetchAndSetOrders` — finding / scheduling

Actual source dispatches its child/xhr callback once before the returned Promise is awaited. Source controls prove this immediate effect. Native body is a lazy future; making/dropping it without polling loses the effect. The alias note does not price invocation/poll/drop timing. Native scheduling is inspected only.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts:434". Native: "sdk-rs/safe-cex-venue-mexc/src/exchange.rs:1244". Source profile: `mexc-fetchAndSetOrders`. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="mexc-7"></a>

### mexc 7. `MexcExchange.fetchAndSetRegularOrders` — finding / scheduling

Actual source dispatches its child/xhr callback once before the returned Promise is awaited. Source controls prove this immediate effect. Native body is a lazy future; making/dropping it without polling loses the effect. The alias note does not price invocation/poll/drop timing. Native scheduling is inspected only.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts:529". Native: "sdk-rs/safe-cex-venue-mexc/src/exchange.rs:1386". Source profile: `mexc-fetchAndSetRegularOrders`. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="mexc-8"></a>

### mexc 8. `MexcExchange.fetchBalance` — finding / existing

Known current causes: direct TS xhr rejection retains Axios provenance while native fetch_balance constructs message-only ExchangeError. Actual normalizeBalance also exposes prototype-backed currencies reads omitted by native map. Alias choice is separately priced; neither cause is newly inferred from the checker.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts:325". Native: "sdk-rs/safe-cex-venue-mexc/src/exchange.rs:778". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-9"></a>

### mexc 9. `MexcExchange.fetchHistoricalFills` — finding / existing

Known current non-finite request gap: TS places opts.from directly in start_time; native json!(from) converts NaN to null at exchange.rs525–527 before transport. The existing note names that actual site. No new runtime evidence or complete fills parity.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts:561". Native: "sdk-rs/safe-cex-venue-mexc/src/exchange.rs:500". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-10"></a>

### mexc 10. `MexcExchange.fetchMarkets` — finding / scheduling

Actual source dispatches its child/xhr callback once before the returned Promise is awaited. Source controls prove this immediate effect. Native body is a lazy future; making/dropping it without polling loses the effect. The alias note does not price invocation/poll/drop timing. Native scheduling is inspected only.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts:298". Native: "sdk-rs/safe-cex-venue-mexc/src/exchange.rs:935". Source profile: `mexc-fetchMarkets`. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="mexc-11"></a>

### mexc 11. `MexcExchange.fetchOHLCV` — unresolved / input-domain

WITHDRAWN current-defect claim: the historical constructor interval witness injects a string outside OHLCVOptions.interval:Timeframe (types.ts623,742). No actual source caller admitting it is established. Preserve the invalid-runtime-input observation without claiming an admitted body defect or inventing an unrecorded input choice.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts:583". Native: "sdk-rs/safe-cex-venue-mexc/src/exchange.rs:425". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#closed-typed-output-boundary).

<a id="mexc-12"></a>

### mexc 12. `MexcExchange.fetchPositionMode` — finding / existing

Known current rejection-provenance gap: TS directly awaits xhr without catch; native execute.map_err constructs ExchangeError::new(error.message), dropping RestFailure metadata. Current note correctly describes that direct caller boundary.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts:335". Native: "sdk-rs/safe-cex-venue-mexc/src/exchange.rs:749". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-13"></a>

### mexc 13. `MexcExchange.fetchPositions` — finding / documentation

Documentation finding: actual TS catch emits err.response.data.message || err.message and returns Store fallback; it does not forward original Axios error metadata to listeners. Native failure_message selects that same response.message/transport.message and emits it. Withdraw the note's claimed metadata-preservation contrast; keep priced info alias choice. No assertion of full error-domain parity.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts:340". Native: "sdk-rs/safe-cex-venue-mexc/src/exchange.rs:608". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="mexc-14"></a>

### mexc 14. `MexcExchange.fetchTickers` — finding / scheduling

Actual source dispatches its child/xhr callback once before the returned Promise is awaited. Source controls prove this immediate effect. Native body is a lazy future; making/dropping it without polling loses the effect. The alias note does not price invocation/poll/drop timing. Native scheduling is inspected only.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts:310". Native: "sdk-rs/safe-cex-venue-mexc/src/exchange.rs:808". Source profile: `mexc-fetchTickers`. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="mexc-15"></a>

### mexc 15. `MexcExchange.getAccount` — finding / existing

Known current validation-detail gap: getAccount awaits validateAccount, then throws its returned error. TS validator selects response.data.message/msg before error.message; native fetch_balance has already discarded response metadata, so the account path can throw transport message instead. Existing account_oracle.mjs fixture directly distinguishes messages.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts:201". Native: "sdk-rs/safe-cex-venue-mexc/src/exchange.rs:271". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-16"></a>

### mexc 16. `MexcExchange.updateSymbolMetadata` — unresolved / verification-gap

Unresolved verification gap, not a newly established behavior defect: both source updateSymbolMetadata and native update_symbol_metadata delegate/await leverage fetch; native propagation exists. Source delegation/subscriber evidence plus native child leverage test do not establish same-method native throwing-subscriber proof. Keep original note, request bounded same-method test before closing.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts:1432". Native: "sdk-rs/safe-cex-venue-mexc/src/exchange.rs:1042". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-17"></a>

### mexc 17. `MexcExchange.validateAccount` — finding / existing

Known current response-detail mismatch: TS validator consumes response.data.message/msg before transport message; native receives only projected error.message from fetch_balance. Existing oracle pins response-detail mismatch. Uncovered fallback/emission cases remain a bounded proof limitation, not a new defect count.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts:190". Native: "sdk-rs/safe-cex-venue-mexc/src/exchange.rs:242". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-18"></a>

### mexc 18. `extractRows` — finding / existing

Known current generic helper gap: data?.[__proto__] on an ordinary array-like source can return Array.prototype and select empty inherited array before later items fallback. Native shared property accessor cannot return that referent. Existing keys[__proto__,items] witness; ordinary production callers orders/items are explicitly unaffected.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts:82". Native: "sdk-rs/safe-cex-venue-mexc/src/exchange_helpers.rs:70". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-19"></a>

### mexc 19. `normalizeBalance` — finding / existing

Known current dictionary-write gap: TS assignment acc[__proto__] installs the currency Balance object as prototype; inherited free/total reads survive in memory. Native skips only that own key and cannot reproduce those reads. Alias ownership and currency ordering are separately priced; no diagnostic-stats exemption applies to real Balance data.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.utils.ts:207". Native: "sdk-rs/safe-cex-venue-mexc/src/utils.rs:320". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="mexc-20"></a>

### mexc 20. `normalizeExecution` — finding / explanation

Actual normalizeExecution returns symbol with materialized D800, directly asserted before serialization. Required native Execution.symbol uses String and cannot retain that unit. Native String/Value ingress refuses the whole mapping record rather than only that field (inspected, not run). The commissionAsset shape choice does not price this distinct returned-value cost under section19.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.utils.ts:503". Native: "sdk-rs/safe-cex-venue-mexc/src/utils.rs:913". Source profile: `mexc-normalizeExecution`. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict), [anchor](../../MANIFEST-SPEC.md#closed-typed-output-boundary).

<a id="mexc-21"></a>

### mexc 21. `normalizeMarket` — finding / explanation

Actual normalizeMarket returns symbol with materialized D800, directly asserted before serialization. Required native Market.symbol uses String and cannot retain that unit. Native String/Value ingress refuses the whole mapping record rather than only that field (inspected, not run). The live-alias ownership choice does not price this distinct returned-value cost under section19.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.utils.ts:77". Native: "sdk-rs/safe-cex-venue-mexc/src/utils.rs:84". Source profile: `mexc-normalizeMarket`. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="mexc-22"></a>

### mexc 22. `normalizeOrder` — finding / explanation

Actual normalizeOrder returns symbol with materialized D800, directly asserted before serialization. Required native Order.symbol uses String and cannot retain that unit. Native String/Value ingress refuses the whole mapping record rather than only that field (inspected, not run). The live-alias ownership choice does not price this distinct returned-value cost under section19.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.utils.ts:343". Native: "sdk-rs/safe-cex-venue-mexc/src/utils.rs:763". Source profile: `mexc-normalizeOrder`. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="mexc-23"></a>

### mexc 23. `normalizePlanOrder` — finding / explanation

Actual normalizePlanOrder returns symbol with materialized D800, directly asserted before serialization. Required native Order.symbol uses String and cannot retain that unit. Native String/Value ingress refuses the whole mapping record rather than only that field (inspected, not run). The live-alias ownership choice does not price this distinct returned-value cost under section19.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.utils.ts:428". Native: "sdk-rs/safe-cex-venue-mexc/src/utils.rs:817". Source profile: `mexc-normalizePlanOrder`. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="mexc-24"></a>

### mexc 24. `normalizePosition` — finding / explanation

Actual normalizePosition returns symbol with materialized D800, directly asserted before serialization. Required native Position.symbol uses String and cannot retain that unit. Native String/Value ingress refuses the whole mapping record rather than only that field (inspected, not run). The live-alias ownership choice does not price this distinct returned-value cost under section19.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.utils.ts:274". Native: "sdk-rs/safe-cex-venue-mexc/src/utils.rs:650". Source profile: `mexc-normalizePosition`. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="mexc-25"></a>

### mexc 25. `normalizeStopOrder` — finding / explanation

Actual normalizeStopOrder returns symbol with materialized D800, directly asserted before serialization. Required native Order.symbol uses String and cannot retain that unit. Native String/Value ingress refuses the whole mapping record rather than only that field (inspected, not run). The live-alias ownership choice does not price this distinct returned-value cost under section19.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.utils.ts:465". Native: "sdk-rs/safe-cex-venue-mexc/src/utils.rs:852". Source profile: `mexc-normalizeStopOrder`. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="mexc-26"></a>

### mexc 26. `normalizeTicker` — finding / explanation

Actual normalizeTicker returns symbol with materialized D800, directly asserted before serialization. Required native Ticker.symbol uses String and cannot retain that unit. Native String/Value ingress refuses the whole mapping record rather than only that field (inspected, not run). The live-alias ownership choice does not price this distinct returned-value cost under section19.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.utils.ts:125". Native: "sdk-rs/safe-cex-venue-mexc/src/utils.rs:503". Source profile: `mexc-normalizeTicker`. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="mexc-27"></a>

### mexc 27. `MexcPrivateWebsocket` — finding / existing

Known current composition gap: TS MexcExchange constructor creates a private websocket; native MexcExchange fields contain no private socket owner or registration of MexcPrivateHandlers. Isolated private_session/ws_lifecycle ports exist and are acknowledged by the note; they do not establish production composition.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts:22". Native: "sdk-rs/safe-cex-venue-mexc/src/ws_private.rs:74". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-28"></a>

### mexc 28. `MexcPrivateWebsocket#applyStopLikeFrame` — unresolved / verification-gap

Unresolved verification gap: source final addOrUpdateOrders and native store.add_or_update_orders? both propagate notification failure after commit; same-method native throwing-subscriber witness was not found. This is not evidence of a current propagation defect; retain original note pending bounded proof.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts:356". Native: "sdk-rs/safe-cex-venue-mexc/src/ws_private.rs:237". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-29"></a>

### mexc 29. `MexcPrivateWebsocket#handleFill` — finding / existing

Known current own-property gap: source handleFill emits orderId:fill.orderId and normalizeExecution supplies empty string when missing; native handle_fill filters empty order_id and omits event.orderId. Secondary documentation issue: absent fee/realizedPnl use toNumber default0 in actual source/native, so the note's NaN claim is stale. Do not count that stale sentence as another row.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts:296". Native: "sdk-rs/safe-cex-venue-mexc/src/ws_private.rs:156". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-30"></a>

### mexc 30. `MexcPrivateWebsocket#handleOrder` — unresolved / verification-gap

Unresolved verification gap: source handleOrder commits addOrUpdateOrders, then terminal statuses and later rejection hooks; native ? propagates at both writes before later work. A same-method native subscriber/sequence witness remains absent in inspected tests, so do not assert a current body defect from that missing proof.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts:253". Native: "sdk-rs/safe-cex-venue-mexc/src/ws_private.rs:101". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-31"></a>

### mexc 31. `MexcPrivateWebsocket#handlePlanOrder` — unresolved / verification-gap

Unresolved verification gap: source handlePlanOrder delegates applyStopLikeFrame and native does likewise, with post-commit propagation. No inspected same-method native throwing-subscriber proof establishes this row. Keep original note rather than relabel a current behavior defect.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts:341". Native: "sdk-rs/safe-cex-venue-mexc/src/ws_private.rs:229". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-32"></a>

### mexc 32. `MexcPrivateWebsocket#handlePosition` — finding / documentation

Documentation finding: FINDING label is stale against its own positive proof. Native tests/ws.rs737–840 invokes handle_position, asserts committed position, returned subscriber id[0] and skipped second listener; actual source handlePosition and native ? commit/propagate at corresponding sites. This is inspected existing test code, not a fresh passed runtime gate.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts:318". Native: "sdk-rs/safe-cex-venue-mexc/src/ws_private.rs:200". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-33"></a>

### mexc 33. `MexcPrivateWebsocket#handleStopOrder` — unresolved / verification-gap

Unresolved verification gap: both source stop wrapper and native delegate to shared path, which propagates Store failure. Same-method native throwing-subscriber evidence is still missing; do not transfer a normal-case wrapper witness or position test as full proof.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts:352". Native: "sdk-rs/safe-cex-venue-mexc/src/ws_private.rs:233". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-34"></a>

### mexc 34. `MexcPublicWebsocket` — finding / existing

Known current public lifecycle/composition gap: native MexcPublicProtocol is not a production MexcExchange socket member; its legacy on_open pings/resubscribes but does not call new isolated public_on_open lifecycle. Note acknowledges new isolated body ports rather than asserting no implementation exists.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.ws-public.ts:18". Native: "sdk-rs/safe-cex-venue-mexc/src/ws_public.rs:41". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-35"></a>

### mexc 35. `MexcPublicWebsocket#addHandler` — finding / existing

Known composition gap for the named receiver path: native helper exists on an isolated protocol, but actual production MexcExchange has no constructed/registered protocol owner corresponding to TS class registry. Do not call this a checker artifact or absence of the helper body. Source copy-versus-native in-place list alias behavior is an additional unproved audit question, not a new asserted defect.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.ws-public.ts:263". Native: "sdk-rs/safe-cex-venue-mexc/src/ws_public.rs:122". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-36"></a>

### mexc 36. `MexcPublicWebsocket#removeHandler` — finding / existing

Known composition gap: isolated native remove_handler exists and performs removal/cleanup, but is not registered as the production TS websocket path. Do not promote the class assembly or claim runtime listener proof.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.ws-public.ts:267". Native: "sdk-rs/safe-cex-venue-mexc/src/ws_public.rs:136". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-37"></a>

### mexc 37. `MexcPublicWebsocket#subscribe` — finding / existing

Known composition gap: native subscribe exists and updates/sends through an injected socket, but no production MexcExchange instantiates/registers its protocol. Source retains subscription object aliases while native owns params; that extra alias cost would need its own actual-source witness before a new classification.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.ws-public.ts:226". Native: "sdk-rs/safe-cex-venue-mexc/src/ws_public.rs:58". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-38"></a>

### mexc 38. `MexcPublicWebsocket#unsubscribe` — finding / existing

Known composition gap: native unsubscribe exists but the production websocket path is not assembled. Own tracking/send behavior is inspected; arbitrary subscription alias/prototype parity is not proved.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.ws-public.ts:233". Native: "sdk-rs/safe-cex-venue-mexc/src/ws_public.rs:81". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-39"></a>

### mexc 39. `MexcPublicWebsocket.constructor` — finding / existing

Known constructor gap: TS constructor invokes super(parent) and registers activeTickerSymbols:changed listener; native MexcPublicProtocol::new initializes only maps/adapter and no production MexcExchange caller installs it. Isolated new_exchange assembly callback does not constitute concrete registration.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.ws-public.ts:27". Native: "sdk-rs/safe-cex-venue-mexc/src/ws_public.rs:49". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="mexc-40"></a>

### mexc 40. `MexcPublicWebsocket.onMessage` — unresolved / verification-gap

Unresolved route-level verification gap: native on_message calls handle_ticker_frame with ? and helper Store update propagates; existing tests/ws.rs737 calls handle_ticker_frame directly, not on_message. Thus the note's route-level witness requirement is still real; do not transfer the helper test as a proved route or assert a current behavior defect.

Source: "packages/safe-cex/src/exchanges/mexc/mexc.ws-public.ts:57". Native: "sdk-rs/safe-cex-venue-mexc/src/ws_public.rs:360". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-1"></a>

### asterdex 1. `generateAsterDEXCodeMessageSignature` — finding / existing

The existing MIXED note correctly owns the shared ethers error-class/identity defect separately from justified private-key redaction. Source throws the original ethers error; native returns Result<String,String>. This is an existing open carrier loss, not a newly discovered cryptographic/signature fault.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.api.ts:86"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/api_helpers.rs:34"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-2"></a>

### asterdex 2. `generateV3Signature` — supported / scope

The four real Rust targets correspond only to source ABI encoding, Keccak, EIP-191 hash and signing operations. The note explicitly excludes payload construction, BigInt conversion and the venue adapter. Support is limited to that honest dependency-operation mapping; it is not function-level completion or public request/signature integration.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.api.ts:136"]. Native: ["sdk-rs/safe-cex-crypto/src/ethereum.rs:70", "sdk-rs/safe-cex-crypto/src/ethereum.rs:92", "sdk-rs/safe-cex-crypto/src/ethereum.rs:97", "sdk-rs/safe-cex-crypto/src/ethereum.rs:102"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-3"></a>

### asterdex 3. `approveAsterBuilderFeeWithBrowserWallet` — finding / explanation

Source returns truthy payload.msg/error without narrowing its value; native converts a non-string into a named failure and eventually a String result. Calling this FORCED because result.message is String is unsupported: a broader result carrier is expressible. Price the chosen carrier, lost returned value and rationale; compact checksum diagnostics likewise do not prove impossibility.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.builder-approval.ts:332"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/builder_approval.rs:352"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#closed-typed-output-boundary).

<a id="asterdex-4"></a>

### asterdex 4. `checkAsterBuilderApproved` — finding / explanation

Compact checksum diagnostics and typed CheckParams are representable error/input policies, not FORCED limitations. Source catches original ethers messages into its returned result. The native signing-before-fetch-catch placement is real, but the note needs separate reasons/costs for its carrier restrictions and any caller-polled scheduling; no exact error or async equivalence is certified.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.builder-approval.ts:220"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/builder_approval.rs:146"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-5"></a>

### asterdex 5. `normalizeAddress` — finding / explanation

Native reproduces source preguard/field-name/zero-address branches around checksum validation, but replaces ethers checksum text with compact shared diagnostics. Shorter diagnostics alone do not establish FORCED, a required surrogate unit, or Rust inability. State a chosen diagnostic/dependency policy or retain an owned error defect; do not infer program observation from the error label alone.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.builder-approval.ts:76"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/builder_approval.rs:103"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-6"></a>

### asterdex 6. `resolveAsterBuilderFeeConfig` — supported / input

Within declared typed ExchangeOptions, fee validation precedes address resolution and zero/zero fees short-circuit it. The address wrapper maps malformed addresses to fixed text; accepted addresses and decimal rates do not establish retained lone-unit output. The common UTF-8 input cost and oversized ICAP reference behavior are honestly priced; arbitrary executable/options-object parity is excluded from this conclusion.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.builder-fee.ts:95"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/builder.rs:410"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#shared-utf8-string-boundary).

<a id="asterdex-7"></a>

### asterdex 7. `resolveBuilderAddress` — supported / input

Trimming, placeholder suppression, fixed checksum-error wrapping and zero rejection match inspected native branches. A surrogate-invalid address is rejected by ethers and replaced with fixed diagnostic text; it is not returned. Support is for the declared UTF-8 input choice only, not a FORCED retained-output claim or arbitrary option objects.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.builder-fee.ts:72"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/builder.rs:366"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#shared-utf8-string-boundary).

<a id="asterdex-8"></a>

### asterdex 8. `AsterDEXExchange#assertV3Feature` — supported / input

The helper delegates emit/report/throw to real BaseExchange.reject_preflight. Current public callers supply closed feature literals. The standalone featureSurrogates fixture therefore does not prove a public SDK retained-unit consequence. Support is limited to the declared typed input boundary and actual BaseExchange dependency, not complete class wiring or a FORCED program-unit claim.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:710"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/exchange_helpers.rs:1018"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#shared-utf8-string-boundary), [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict).

<a id="asterdex-9"></a>

### asterdex 9. `AsterDEXExchange#extractFilterValue` — supported / input

First-match stopping, null filter failure, truthiness and shared inherited lookup are represented. The helper returns a number; admitting a surrogate field/type input does not imply a retained output unit. Public transformMarket callers use fixed minQty/maxQty fields. This supports the bounded ordinary-filter/input rationale without claiming executable arrays or a public surrogate consequence.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:1510"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/exchange_helpers.rs:811"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#shared-utf8-string-boundary).

<a id="asterdex-10"></a>

### asterdex 10. `AsterDEXExchange#fetchOpenInterestMap` — unresolved / explanation

The existing private-helper fixture records a one-entry Map from a JSON-decoded D800 symbol, while native String/Value refuses that ingress. It records typeof/size but not an actual public consumer of the materialized key. A section19/whole-map explanation finding needs that caller observation first. This remains an unresolved verification boundary, not a newly measured body defect.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:1370"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/exchange_helpers.rs:76"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#shared-utf8-string-boundary), [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict).

<a id="asterdex-11"></a>

### asterdex 11. `AsterDEXExchange#pollOpenInterest` — supported / scheduling choice

The note explicitly prices owner cancellation and omission of loop.ts visibility/requestAnimationFrame and nonvisible zero-delay dispatch. Source finally registers loop; native registers an owner timeout with weak continuation. These are disclosed choices, not exact host-turn/drop equivalence. This review adds no ready-resolution, browser-host or disposal execution proof. Checker shape analogue: order_execution.rs:3 private types::* alongside local OrderExecution(:60)/impl(:73); no pub use and no measured checker rejection.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:1449"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/order_execution.rs:548"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-12"></a>

### asterdex 12. `AsterDEXExchange#reverseMapOrderStatus` — finding / documentation

The claim that closed Order.status consumption belongs to a pending mapper is stale: transform_order consumes this helper and fetch_orders calls the mapper. The helper preserves inherited TableHit values; unknown lone-unit status falls back to Open, so no retained-unit output follows. Correct the scope statement without inventing a new lookup/body defect.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:1608"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/exchange_helpers.rs:766"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#shared-utf8-string-boundary), [anchor](../../MANIFEST-SPEC.md#closed-typed-output-boundary).

<a id="asterdex-13"></a>

### asterdex 13. `AsterDEXExchange#reverseMapOrderType` — finding / documentation

The note calls the closed Order.type consumer a separate pending mapper, but transform_order and fetch_orders already exist. This scope statement is inaccurate independently of surrogate observability. A public fetchOrders witness with a complete order carrying D800 type remains necessary before escalating the generic UTF-8 rationale under section19.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:1594"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/exchange_helpers.rs:791"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#shared-utf8-string-boundary), [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict), [anchor](../../MANIFEST-SPEC.md#closed-typed-output-boundary).

<a id="asterdex-14"></a>

### asterdex 14. `AsterDEXExchange#startOpenInterestPolling` — unresolved / scheduling

Owner cancellation is expressly priced, but native immediate-start is not proved: start_owned_operation delegates through OwnedTasks.start to runtime.spawn_local, with no eager-poll requirement. The current test runtime queues the future, and the witness pumps before checking requests. This is an unresolved eager-prefix/host-contract proof gap, not a newly executed scheduling defect. Checker shape analogue: order_execution.rs:3 private types::* alongside local OrderExecution(:60)/impl(:73); no pub use and no measured checker rejection.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:1486"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/order_execution.rs:597"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-15"></a>

### asterdex 15. `AsterDEXExchange#syncPositionMode` — finding / explanation

The shared REST boundary really loses the source AxiosError name/identity as the note says, but FORCED is unsupported: ExchangeError has a name String and broader carriers are possible. Price the shared structural ABI/reason or retain the existing defect; caller polling and configured API construction are separate boundaries. No new error execution is claimed. Checker shape analogue: account_fetch.rs:4 private types::* alongside local AccountFetch(:14)/impl(:34); no pub use. Current unconditional-local-nominal bypass means no checker artifact is established.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:400"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/account_fetch.rs:152"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-16"></a>

### asterdex 16. `AsterDEXExchange#transformMarket` — finding / existing

The note correctly separates owned-info alias cost from an existing admitted-number loss: source returns info:market; native serde_json::to_value(market) erases native nonfinite metadata. Existing test code independently sets native precision nonfinites and checks null metadata. This mapper remains unrepaired; no new precision-axis or public surrogate fault is asserted.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:1315"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/exchange_helpers.rs:849"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-17"></a>

### asterdex 17. `AsterDEXExchange#transformOrder` — finding / existing

Source retains raw order numeric metadata in info; native serde_json::to_value erases admitted nonfinite orderId/time/updateTime/workingTime. This is already disclosed and separately priced from alias and closed-enum refusal. Reconcile the special __proto__ wording with current section15 rather than counting it automatically as another new defect. No new native measurement is added.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:1293"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/exchange_helpers.rs:914"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#closed-typed-output-boundary), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-18"></a>

### asterdex 18. `AsterDEXExchange#transformPosition` — finding / existing

Source returns info:pos; native constructs owned info with serde_json::to_value(pos), erasing native nonfinite updateTime. The note already owns this metadata loss separately from bidirectional alias cost. Native derived f64 arithmetic can remain nonfinite while info loses it. No new runtime or full REST/Store composition proof is claimed.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:1259"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/exchange_helpers.rs:381"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-19"></a>

### asterdex 19. `AsterDEXExchange#transformTicker` — finding / existing

Source returns info:ticker; native serializes the typed ticker into Value, erasing native nonfinite closeTime. Existing fixtures describe this before wire normalization. The note already owns the numeric defect and separately prices snapshots; this is not a newly discovered OI/arithmetic issue or an executed public unit finding.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:1490"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/exchange_helpers.rs:352"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-20"></a>

### asterdex 20. `AsterDEXExchange.cancelOrders` — finding / explanation

Source DELETE config includes __suppressAsterUnknownOrderError; native receives _suppress but drops it because RestRequest lacks that carrier. The difference is named but not FORCED: transport metadata is expressible. Price the shared request/interceptor policy and outgoing logging cost; request-field absence alone does not prove the complete interceptor/user-visible effect. Checker shape analogue: order_execution.rs:3 private types::* alongside local OrderExecution(:60)/impl(:73); no pub use and no measured checker rejection.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:1043"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/order_execution.rs:410"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-21"></a>

### asterdex 21. `AsterDEXExchange.changePositionMode` — finding / explanation

Source and native both compose POST, setting/log, then refresh with swallowed refresh errors. Strict DTO admission and owned positions are chosen models, not FORCED. Price the actual nested-warning/Store-update cost and why, alongside separately named polling. No new refresh/catch-order defect is established. Checker shape analogue: account_fetch.rs:4 private types::* alongside local AccountFetch(:14)/impl(:34); no pub use. Current unconditional-local-nominal bypass means no checker artifact is established.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:645"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/account_fetch.rs:503"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="asterdex-22"></a>

### asterdex 22. `AsterDEXExchange.fetchMarkets` — finding / explanation

Complete trading-market DTO admission and numeric-only leverage brackets are chosen typed models, not language impossibilities. Source can retain string initialLeverage in output while native refuses it. Owned info and caller polling are named but need an appropriate chosen rationale instead of FORCED. No complete coercion/numeric-domain proof is supplied by inspection. Checker shape analogue: account_fetch.rs:4 private types::* alongside local AccountFetch(:14)/impl(:34); no pub use. Current unconditional-local-nominal bypass means no checker artifact is established.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:497"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/account_fetch.rs:301"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="asterdex-23"></a>

### asterdex 23. `AsterDEXExchange.fetchOrders` — finding / explanation

Strict selected-row DTOs, whole-record closed OrderType refusal, owned info and structural errors are explicitly described. Section15 calls closed enum output CHOSEN, never FORCED. Correct/price the bucket without claiming source enum correctness or serialized convergence. Empty/filter branch inspection is not a new executed gate. Checker shape analogue: account_fetch.rs:4 private types::* alongside local AccountFetch(:14)/impl(:34); no pub use. Current unconditional-local-nominal bypass means no checker artifact is established.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:483"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/account_fetch.rs:258"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#closed-typed-output-boundary), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="asterdex-24"></a>

### asterdex 24. `AsterDEXExchange.fetchPositions` — finding / explanation

Native decodes all position DTOs before symbol/zero filtering; source may ignore missing unused fields. This named acceptance and owned-info difference is a chosen model, not FORCED. Price whole-response refusal versus source positions/placeholders and explain polling separately; no new leverage/placeholder algorithm fault is demonstrated. Checker shape analogue: account_fetch.rs:4 private types::* alongside local AccountFetch(:14)/impl(:34); no pub use. Current unconditional-local-nominal bypass means no checker artifact is established.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:410"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/account_fetch.rs:172"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="asterdex-25"></a>

### asterdex 25. `AsterDEXExchange.fetchPublicTrades` — supported / closed output

Actual source casts uppercase BUY/SELL into the closed OrderSide; the native per-trade mapper refuses the out-of-enum value. The note cites section15, states no PublicTrade record is returned, and excludes network/logging composition. This is an honest partial closed-output mapping, not the complete fetchPublicTrades body or serialized convergence.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:597"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/exchange_helpers.rs:981"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#closed-typed-output-boundary).

<a id="asterdex-26"></a>

### asterdex 26. `AsterDEXExchange.fetchTickers` — finding / explanation

Strict ticker DTOs, owned info, structural REST errors and polling are already named but incorrectly collected under FORCED. These are expressible input/alias/error/lifecycle decisions needing reasons and costs. Bulk OI startup control flow is inspected; generic String does not establish a retained public unit or a new request-order fault. Checker shape analogue: account_fetch.rs:4 private types::* alongside local AccountFetch(:14)/impl(:34); no pub use. Current unconditional-local-nominal bypass means no checker artifact is established.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:577"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/account_fetch.rs:392"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="asterdex-27"></a>

### asterdex 27. `AsterDEXExchange.placeOrder` — finding / explanation

The finite-progress guard is an explicit justified CHOSEN departure from a potentially nonterminating split. The accompanying FORCED typed ExecutionOrder ingress and Axios identity clause is still a representable carrier/input choice. Separate those causes and any eager-preflight/drop cost. No new split algorithm or successful-order fault is asserted. Checker shape analogue: order_execution.rs:3 private types::* alongside local OrderExecution(:60)/impl(:73); no pub use and no measured checker rejection.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:742"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/order_execution.rs:176"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#closed-typed-output-boundary).

<a id="asterdex-28"></a>

### asterdex 28. `AsterDEXExchange.placeOrders` — finding / explanation

The row honestly inherits finite-progress rejection and states how it lets later batch items run. Caller-polled futures are not FORCED: eager owned task/observer models are expressible. Price this API/lifecycle choice separately from finite-progress behavior; this review adds no partial-ID/continuation proof. Checker shape analogue: order_execution.rs:3 private types::* alongside local OrderExecution(:60)/impl(:73); no pub use and no measured checker rejection.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:970"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/order_execution.rs:338"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-29"></a>

### asterdex 29. `AsterDEXExchange.setLeverage` — finding / explanation

Source rejects nonfinite leverage input but can create a nonfinite clamped value from market bounds and pass it to Axios; native refuses that request Value. This is a shared carrier decision with a withheld-POST cost, not Rust inability to retain f64. Price that choice independently of error identity/polling and do not infer preservation from JSON null. Checker shape analogue: account_fetch.rs:4 private types::* alongside local AccountFetch(:14)/impl(:34); no pub use. Current unconditional-local-nominal bypass means no checker artifact is established.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:617"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/account_fetch.rs:436"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="asterdex-30"></a>

### asterdex 30. `AsterDEXExchange.updateOrder` — finding / explanation

Cancel-before-replace is implemented, but missing suppression config and strict UpdateOrderOpts null/undefined admission are representable shared input/carrier policies, not FORCED. State their why and inherited old-order interceptor cost separately. No new cancel/replace ordering bug is established. Checker shape analogue: order_execution.rs:3 private types::* alongside local OrderExecution(:60)/impl(:73); no pub use and no measured checker rejection.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:1000"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/order_execution.rs:375"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-31"></a>

### asterdex 31. `AsterDEXExchange.validateAccount` — finding / explanation

Actual source validateAccount rethrows its AxiosError after nested getAccount logging; native structural failure has name Error. The existing carrier loss is described, but FORCED does not establish Rust inability. Price the shared error ABI/reason or retain an owned defect; full constructor/auth/live traffic is not covered. Checker shape analogue: order_execution.rs:3 private types::* alongside local OrderExecution(:60)/impl(:73); no pub use and no measured checker rejection.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts:353"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/order_execution.rs:129"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-32"></a>

### asterdex 32. `AsterDEXAccountResponse` — finding / existing

Ordinary direct raw-Serde f64 decoding retains the already known overflow refusal on feeTier/updateTime and nested timestamps. AccountResponse and nested JsNumberOutput opt-in registrations exist, but ordinary production decode remains serde_json::from_value. Opt-in acceptance is not global consumer rewiring, and this existing finding is not a new fault measured here.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts:196"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/types.rs:770"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-33"></a>

### asterdex 33. `AsterDEXAccountUpdate` — finding / existing

Source AccountUpdate E/T are numbers; ordinary raw typed f64 decoding retains the existing overflow refusal. The native shape and nested records remain ordinary Deserialize rather than an opted-in shared entry point. Existing numeric test code asserts native values/refusals separately from wire output. No new execution or all-caller raw-decoding claim is made.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts:511"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/types.rs:2290"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-34"></a>

### asterdex 34. `AsterDEXDepthResponse` — finding / existing

DepthResponse lastUpdateId/E/T remain ordinary f64 fields and source number declarations admit JSON overflow. The already named ordinary raw-overflow defect remains; no local native shared-ingress opt-in is present for this shape. This is existing contract work, not a newly discovered depth-handler fault or proof of every runtime caller.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts:326"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/types.rs:1430"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-35"></a>

### asterdex 35. `AsterDEXKlineResponse` — finding / existing

The KlineResponse tuple uses native f64 slots0/6/8 corresponding to source number entries. Its ordinary tuple Deserialize retains the existing raw-overflow refusal. The note does not claim a venue decoder repair; no new kline producer/consumer execution or complete numeric-domain proof is made.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts:430"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/types.rs:1709"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-36"></a>

### asterdex 36. `AsterDEXMarketResponse` — finding / existing

MarketResponse numeric precision/optional fields and nested filter numbers still pass ordinary f64 Deserialize. Existing raw-overflow refusal remains separate from any admitted-number info loss in transformMarket. The note already owns this defect; no new runtime or all-market consumer claim is asserted.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts:158"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/types.rs:354"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-37"></a>

### asterdex 37. `AsterDEXOpenInterestResponse` — finding / documentation

The ordinary overflow finding remains, but 'this shape is not yet opted in' is stale: types.rs:1423 implements JsNumberOutput for OpenInterestResponse and live_types.rs:1735 checks optional time on shared raw/owned/borrowed paths. Correct the opt-in scope wording; ordinary Deserialize consumers remain unchanged, so this is not a promotion claim.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts:320"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/types.rs:1384"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-38"></a>

### asterdex 38. `AsterDEXOrderResponse` — finding / documentation

The claimed opt-in repair of id/time belongs to TradeResponse, not OrderResponse. OrderResponse has orderId/time/updateTime/workingTime and no JsNumberOutput implementation; live_types.rs:1740/1745 tests TradeResponse.id/time. Ordinary OrderResponse overflow remains existing/open. The copied successful-repair attribution is unsupported for this row.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts:274"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/types.rs:1064"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-39"></a>

### asterdex 39. `AsterDEXOrderUpdate` — finding / existing

OrderUpdate outer E/T and nested numeric o fields retain the named ordinary raw-Serde overflow defect. Source number fields and native f64 declarations are real; this shape is not registered for shared opt-in output. Existing test code is inspected, not executed, and no entire private handler conformance is claimed.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts:537"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/types.rs:2528"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-40"></a>

### asterdex 40. `AsterDEXPlaceOrderOpts` — finding / existing

The flattened PlaceOrderOpts ordinary decoder still retains the named raw overflow/decimal refusal and integer-lexeme -0 rebuffer loss. Its separate JsNumberOutput registration/shared opt-in tests do not change the ordinary flattened Deserialize. This is existing ingress work, not a new placement behavior defect or an executed repair claim.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts:25"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/types.rs:2583"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-41"></a>

### asterdex 41. `AsterDEXPositionResponse` — finding / documentation

The note claims opt-in repair of time, but PositionResponse's numeric field is updateTime (types.rs:996), with no JsNumberOutput implementation. Optional time opt-in belongs to OpenInterestResponse at live_types.rs:1735. PositionResponse's ordinary raw overflow remains open; the wrong-type successful-repair attribution is documentation, not closure.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts:255"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/types.rs:964"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-42"></a>

### asterdex 42. `AsterDEXStreamDepth` — finding / existing

StreamDepth E/T/U/u/pu are ordinary native f64 fields corresponding to source number declarations. Raw overflow refusal is an existing explicitly owned defect; no output registration repairs this shape. Native type inspection does not establish which socket consumers take raw/Value paths or claim a newly executed book fault.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts:474"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/types.rs:1921"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-43"></a>

### asterdex 43. `AsterDEXStreamMarkPrice` — finding / existing

StreamMarkPrice E/T remain ordinary f64 Deserialize fields; the note already owns raw-overflow refusal compared with source number/JSON.parse. No shared opt-in output registration exists for this shape. This review adds no live handler, scheduling or full numeric-domain execution.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts:499"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/types.rs:2092"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-44"></a>

### asterdex 44. `AsterDEXStreamTicker` — finding / existing

StreamTicker E/O/C/F/L/n remain ordinary f64 Deserialize fields. Their ordinary raw-overflow refusal is already named, not newly discovered. No shared output registration repairs this shape; native declarations/test expectations alone do not prove all stream consumers or new runtime effects.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts:453"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/types.rs:1793"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-45"></a>

### asterdex 45. `AsterDEXStreamTrade` — finding / existing

StreamTrade E/a/f/l/T retain the ordinary raw-Serde overflow refusal documented in the note. The source number fields and native f64 path are inspected, with no shape-specific shared output registration. This is existing ingress work, not a new aggregate-trade handler defect or an executed native gate.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts:486"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/types.rs:2004"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-46"></a>

### asterdex 46. `AsterDEXTickerResponse` — finding / documentation

The note attributes opt-in id/orderId/time repairs to TickerResponse, but its fields are openTime/closeTime/firstId/lastId/count, and no TickerResponse JsNumberOutput impl exists. live_types.rs:1750–1764 tests UserTradeResponse.id/orderId/time. Ordinary TickerResponse overflow remains existing/open; the successful repair claim belongs to another record.

Source: ["packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts:301"]. Native: ["sdk-rs/safe-cex-venue-asterdex/src/types.rs:1284"]. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-47"></a>

### asterdex 47. `AsterDEXTradeResponse` — finding / existing

The ordinary Wire f64 decoder retains the already-recorded raw-overflow refusal. Its opt-in JsNumberOutput and trade_response_numeric_ingress_acceptance now exist, so the separate not-yet-opted-in sentence is stale; do not call this a newly discovered defect.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts:334-341". Native: "sdk-rs/safe-cex-venue-asterdex/src/types.rs:1470-1528; tests/live_types.rs:1318-1327,1733-1749". Policy: [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-48"></a>

### asterdex 48. `AsterDEXUserTradeResponse` — finding / existing

Ordinary f64 Wire still records known raw-overflow loss; opt-in acceptance now covers id/orderId/time, contradicting only the not-opted-in sentence, not proving ordinary Serde repaired.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts:343-361". Native: "sdk-rs/safe-cex-venue-asterdex/src/types.rs:1531-1685; tests/live_types.rs:1328-1342,1750-1764". Policy: [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-49"></a>

### asterdex 49. `AsterDEXWebSocketMessage` — finding / existing

Known ordinary raw numeric overflow for event-time E remains represented by Wire f64. The existing numeric audit names E explicitly; no new behavior is inferred from generic strings.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts:446-451". Native: "sdk-rs/safe-cex-venue-asterdex/src/types.rs:1725-1777; tests/live_types.rs:1358-1362". Policy: [anchor](../../MANIFEST-SPEC.md#numeric-ingress-defect).

<a id="asterdex-50"></a>

### asterdex 50. `AsterDEXVirtualClock` — supported / None

Note names explicit owner, cancellation on owner disposal and requirement to retain native realm; those costs match ClockOperation and launch implementation. No newly missing rationale.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.virtualClock.ts:16-23,202". Native: "sdk-rs/safe-cex-venue-asterdex/src/virtual_clock.rs:26-34,68-102". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-51"></a>

### asterdex 51. `AsterDEXVirtualClock#syncClientServerTimeInner` — supported / None

Both commit server/client before console; native console callback is infallible while JS replacement can throw afterward. The note prices this precise host-contract cost.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.virtualClock.ts:94-111". Native: "sdk-rs/safe-cex-venue-asterdex/src/virtual_clock.rs:173-200". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-52"></a>

### asterdex 52. `AsterDEXVirtualClock.forceSyncAndGetOffset` — supported / None

Immediate shared-sync dispatch and offset read after completion match; owner cancellation and infallible console residuals are explicitly priced, with owner-disposal existing test.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.virtualClock.ts:178-184". Native: "sdk-rs/safe-cex-venue-asterdex/src/virtual_clock.rs:254-264; tests/live_clock_lifecycle.rs:442". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-53"></a>

### asterdex 53. `AsterDEXVirtualClock.start` — supported / None

Source generation guard and replacement interval sequence match; native fallible interval registration and owner cancellation are expressly chosen and explained.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.virtualClock.ts:28-45". Native: "sdk-rs/safe-cex-venue-asterdex/src/virtual_clock.rs:106-137". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-54"></a>

### asterdex 54. `AsterDEXVirtualClock.syncClientServerTime` — supported / None

The note explicitly retains eager dispatch/coalescing and names unsettled completion/uncleared slot on native owner cancellation. No lazy async-prefix defect: method is a regular fn invoking inner immediately.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.virtualClock.ts:81-92". Native: "sdk-rs/safe-cex-venue-asterdex/src/virtual_clock.rs:150-171". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-55"></a>

### asterdex 55. `asterDEXVirtualClock` — supported / None

One Rc clock per explicit realm versus eager per-module singleton is a precise, reasoned ownership boundary; same-realm sharing remains real.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.virtualClock.ts:202". Native: "sdk-rs/safe-cex-venue-asterdex/src/virtual_clock.rs:291-304". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-56"></a>

### asterdex 56. `AsterDEXPrivateWebsocket` — supported / None

PrivateStream really installs five handlers and combines listen-key/repair/socket capabilities. Explicit native host-object identity/lifetime cost is priced; complete exchange startup remains a separate claim.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts:22-73". Native: "sdk-rs/safe-cex-venue-asterdex/src/private_stream.rs:176-238". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-57"></a>

### asterdex 57. `AsterDEXPrivateWebsocket#applyBalanceUpdates` — finding / explanation

FORCED is unjustified for ordinary-object prototype mutation and closed declared fields: Rust can model these using explicit prototype/extra-field state. Existing native own-data-entry cost is real, but its bucket/rationale needs correction; this review does not claim a new production defect.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts:399-458". Native: "sdk-rs/safe-cex-venue-asterdex/src/private_stream.rs:565-628". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#closed-typed-output-boundary).

<a id="asterdex-58"></a>

### asterdex 58. `AsterDEXPrivateWebsocket#fetchListenKey` — finding / explanation

Actual fetchListenKey feeds its returned D800 key through actual public connectAndSubscribe into createWebsocket URL; last URL unit is D800. Required ListenKeyLifecycle String cannot retain it and the complete key/request is unavailable, not merely a malformed-object exclusion. The CHOSEN-only note does not price this program-observed output under section19.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts:595-608,76-98". Native: "sdk-rs/safe-cex-venue-asterdex/src/listen_key.rs:15-17,88-103; private_stream.rs:252-282". Source profile: `aster-listen-key-consumer`. Policy: [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict), [anchor](../../MANIFEST-SPEC.md#shared-utf8-string-boundary).

<a id="asterdex-59"></a>

### asterdex 59. `AsterDEXPrivateWebsocket#handleAccountPositionUpdate` — supported / None

Live-array versus borrowed owned-snapshot emission and closed currency/prototype residuals are explicitly identified. Real state/version/repair application exists; no plan-only missing body inferred.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts:470-581". Native: "sdk-rs/safe-cex-venue-asterdex/src/private_stream.rs:640-788". Policy: [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [anchor](../../MANIFEST-SPEC.md#closed-typed-output-boundary).

<a id="asterdex-60"></a>

### asterdex 60. `AsterDEXPrivateWebsocket#handleAccountUpdate` — finding / documentation

Helper target still returns a plan, but private_stream now performs parsed balance application and emits account data directly. The broad runtime-wiring-pending annotation is stale; do not count a newly open account-update body omission. Source and mapped helper remain distinct coverage claims.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts:461-468". Native: "sdk-rs/safe-cex-venue-asterdex/src/account_helpers.rs:186-204; private_stream.rs:629-638". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-61"></a>

### asterdex 61. `AsterDEXPrivateWebsocket#handleExecutionReport` — finding / documentation

Closed status cost remains real and the mapped helper returns a patch. PrivateStream now implements scheduled reconciliation, real Store write and emission for this source event, so the historical pending-application inventory must be reconciled; not a new unimplemented runtime body.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts:171-198". Native: "sdk-rs/safe-cex-venue-asterdex/src/account_helpers.rs:13-69; private_stream.rs:417-429,385-413". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#closed-typed-output-boundary).

<a id="asterdex-62"></a>

### asterdex 62. `AsterDEXPrivateWebsocket#handleOrderTradeUpdate` — supported / None

Real Store mutation and cumulative fill preceding execution emission exist. Closed order fields and owned emission snapshots are expressly priced, rather than pretending arbitrary JS identity survives.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts:200-290". Native: "sdk-rs/safe-cex-venue-asterdex/src/private_stream.rs:432-562". Policy: [anchor](../../MANIFEST-SPEC.md#closed-typed-output-boundary), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="asterdex-63"></a>

### asterdex 63. `AsterDEXPrivateWebsocket#mirrorCumulativeOrderFill` — finding / documentation

The pure helper returns a patch as stated, but PrivateStream consumes it with real Store.update_order before fill emission. Pending-runtime narrative has a landed consumer and must be updated; existing helper extraction is not a newly open production mutation omission.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts:292-322". Native: "sdk-rs/safe-cex-venue-asterdex/src/account_helpers.rs:82-125; private_stream.rs:504-518". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-64"></a>

### asterdex 64. `AsterDEXPrivateWebsocket#runBalanceReconcile` — supported / None

Source finally releases busy and reschedules queued current-version work. Native ResetFlight cancellation and structured rejection report costs are explicitly named and priced, no newly undocumented scheduling difference.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts:797-815". Native: "sdk-rs/safe-cex-venue-asterdex/src/private_repairs.rs:187-221". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-65"></a>

### asterdex 65. `AsterDEXPrivateWebsocket#runOrderReconcile` — supported / None

Source allows overlap then checks disposal, replaces orders and loaded.orders and catches failures. Native performs those operations and prices cancellation/closed records/error reporting.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts:834-850". Native: "sdk-rs/safe-cex-venue-asterdex/src/private_repairs.rs:252-291". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#closed-typed-output-boundary).

<a id="asterdex-66"></a>

### asterdex 66. `AsterDEXPrivateWebsocket#runPositionRepair` — supported / None

Dispatch and busy/version prefix are eager in regular fn; real Store operations follow result. Cancellation resetting busy and structured closed records are expressly priced.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts:744-777". Native: "sdk-rs/safe-cex-venue-asterdex/src/private_repairs.rs:123-158". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#closed-typed-output-boundary).

<a id="asterdex-67"></a>

### asterdex 67. `AsterDEXPrivateWebsocket#scheduleBalanceReconcile` — supported / None

Queued-before-guard and first-version capture are actual code; fallible native owner-bound timer is explicitly justified instead of unpriced promise timing.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts:785-795". Native: "sdk-rs/safe-cex-venue-asterdex/src/private_repairs.rs:170-185". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-68"></a>

### asterdex 68. `AsterDEXPrivateWebsocket#scheduleConnectRetry` — supported / None

Native timer clears slot then invokes connect eagerly and launches owned task. Cancellation and native exception reporting residuals are explicitly named and explained.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts:704-726". Native: "sdk-rs/safe-cex-venue-asterdex/src/listen_key.rs:224-253". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-69"></a>

### asterdex 69. `AsterDEXPrivateWebsocket#scheduleListenKeyRefresh` — supported / None

Source arbitrary delay coercion versus u64/fallible bounded timer and owner lifetime is explicitly priced. Replacement cancellation and retained fired slot agree with described bodies.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts:655-665". Native: "sdk-rs/safe-cex-venue-asterdex/src/listen_key.rs:160-181". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-70"></a>

### asterdex 70. `AsterDEXPrivateWebsocket#scheduleOrderReconcile` — supported / None

Native disposed/cancel-old/log/new10s order and clearing slot before fetch match source; owner monotonic fallibility/cancellation is explicit.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts:821-832". Native: "sdk-rs/safe-cex-venue-asterdex/src/private_repairs.rs:235-250". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-71"></a>

### asterdex 71. `AsterDEXPrivateWebsocket#schedulePositionRepair` — supported / None

Disposed guard, queued-before-busy, log and 1s clearing callback are present; note gives an explicit reason for native owned monotonic registration.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts:728-742". Native: "sdk-rs/safe-cex-venue-asterdex/src/private_repairs.rs:103-121". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-72"></a>

### asterdex 72. `AsterDEXPrivateWebsocket#startListenKeyWatchdog` — supported / None

Duplicate-start guard and strict greater-than70min condition are present. Disposal cancellation and fallible native log/timer seams are priced as ownership costs.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts:667-695". Native: "sdk-rs/safe-cex-venue-asterdex/src/listen_key.rs:182-215". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-73"></a>

### asterdex 73. `AsterDEXPrivateWebsocket#updateListenKey` — supported / None

Immediate disposed/key guards and request dispatch are present; structured-error identity and cancellation costs are explicitly chosen and explained. No unpaired-string inference needed for ruling.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts:610-653". Native: "sdk-rs/safe-cex-venue-asterdex/src/listen_key.rs:106-158". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-74"></a>

### asterdex 74. `AsterDEXPrivateWebsocket.connectAndSubscribe` — supported / None

Listen-key request is dispatched before returned future is polled. Native awaited fallible dial, polling and cancellation costs are expressly stated.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts:76-106". Native: "sdk-rs/safe-cex-venue-asterdex/src/private_stream.rs:252-297". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-75"></a>

### asterdex 75. `AsterDEXPrivateWebsocket.dispose` — supported / None

Reset and six timer clears precede BaseWebSocket.dispose; permanent socket owner shutdown versus unfinished JS promises/manual callbacks is explicitly priced.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts:858-876". Native: "sdk-rs/safe-cex-venue-asterdex/src/private_stream.rs:790-796; listen_key.rs:59-66". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-76"></a>

### asterdex 76. `AsterDEXPrivateWebsocket.onMessage` — finding / explanation

Actual private onMessage substring(0,200) cuts at materialized D83D at summary index199; actual BaseExchange.log forwards that final retained unit through the program log event. Native private_stream.rs353–356 String::from_utf16_lossy substitutes FFFD into the required String summary (inspected; no new native run). Historical FORCED substitution note lacks the actual consumer/exact unit/required surface under section19; it cannot assume human-only scope.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts:134-168". Native: "sdk-rs/safe-cex-venue-asterdex/src/private_stream.rs:299-382; tests/live_private_stream.rs:925-940". Source profile: `aster-private-log-cutoff`. Policy: [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict), [anchor](../../MANIFEST-SPEC.md#diagnostic-prototype-write-choice).

<a id="asterdex-77"></a>

### asterdex 77. `AsterDEXPublicWebsocket` — supported / None

Four real handlers and concrete subscription/socket ownership exist. Cancellation of unfinished open/kline tasks is a precise and explained owner-lifetime residual.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts:29-66". Native: "sdk-rs/safe-cex-venue-asterdex/src/public_stream.rs:107-156; public_subscriptions.rs:441-469". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-78"></a>

### asterdex 78. `AsterDEXPublicWebsocket#handleDepthSnapshot` — finding / documentation

public_depth_snapshot stays pure, but PublicStream.depth now applies bid/ask Store patch and delivers cloned listener books. Note claiming absence of these source effects is stale at runtime; not a new market-data body defect.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts:335-403". Native: "sdk-rs/safe-cex-venue-asterdex/src/ws_helpers.rs:35-96; public_stream.rs:426-442; public_subscriptions.rs:402-423". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-79"></a>

### asterdex 79. `AsterDEXPublicWebsocket#handleEventData` — finding / documentation

The mapped dispatch helper is now called by PublicStream.event_data inside on_message batch catch boundary; inherited failure routes through dispatch and aborts remaining batch. Explicit not-wired sentence is stale.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts:204-264,308-315". Native: "sdk-rs/safe-cex-venue-asterdex/src/ws_helpers.rs:115-142; public_stream.rs:231-244,254-272". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-80"></a>

### asterdex 80. `AsterDEXPublicWebsocket#handleKlineUpdate` — finding / documentation

PublicStream.kline now consumes returned stream/candle and PublicSubscriptions.deliver_kline invokes the registered callback. Pure helper extraction remains real, but absence-of-runtime-callback rationale is stale.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts:442-463". Native: "sdk-rs/safe-cex-venue-asterdex/src/ws_helpers.rs:161-209; public_stream.rs:444-450; public_subscriptions.rs:424-438". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-81"></a>

### asterdex 81. `AsterDEXPublicWebsocket#handleMarkPriceUpdate` — finding / documentation

PublicStream.mark consumes helper, calls patch_ticker, then emits markPrice. Runtime Store-before-emission omission is no longer open, though pure mapped helper alone remains plan-valued. Separate closed fundingTime/emitted alias costs are elsewhere priced.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts:423-440". Native: "sdk-rs/safe-cex-venue-asterdex/src/ws_helpers.rs:326-373; public_stream.rs:367-395,306-341". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="asterdex-82"></a>

### asterdex 82. `AsterDEXPublicWebsocket#handleStreamData` — supported / None

Note gives exact materialized D800, source symbol assignment/forwarding consumer, required Value/String ingress and whole-frame refusal, with actual-source supplement citation. It satisfies site-specific surrogate accounting without generalizing Rust inability.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts:266-305". Native: "sdk-rs/safe-cex-venue-asterdex/src/ws_helpers.rs:379-431; public_stream.rs:273-305". Policy: [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict).

<a id="asterdex-83"></a>

### asterdex 83. `AsterDEXPublicWebsocket#handleTickerUpdate` — finding / documentation

PublicStream.ticker now applies patch_ticker then emit_ticker. Old helper-only absence-of-Store/emission finding must be reconciled with implemented consumer; no new native body omission inferred.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts:317-333". Native: "sdk-rs/safe-cex-venue-asterdex/src/ws_helpers.rs:284-323; public_stream.rs:343-365". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="asterdex-84"></a>

### asterdex 84. `AsterDEXPublicWebsocket#handleTradeUpdate` — finding / documentation

PublicStream.trade now constructs event, applies Store last-price patch and emits trade. It duplicates source mapping rather than invoking pure helper, so coverage mapping/old missing-effects narrative needs update; not a newly unimplemented trade body.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts:405-421". Native: "sdk-rs/safe-cex-venue-asterdex/src/ws_helpers.rs:228-251; public_stream.rs:397-423". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-85"></a>

### asterdex 85. `AsterDEXPublicWebsocket#subscribeOrderBook` — supported / None

Real retry guard/identity/count<=50 semantics and advance-before-send are present; fallible100ms owner timer versus JS timeout is priced and justified.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts:489-510". Native: "sdk-rs/safe-cex-venue-asterdex/src/public_subscriptions.rs:248-281". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-86"></a>

### asterdex 86. `AsterDEXPublicWebsocket#unsubscribeOrderBookListener` — supported / None

Last-listener removal/cancel/topic/stats order matches; native reports unsubscribe failure explicitly instead of source unhandled rejection, with reason given.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts:512-530". Native: "sdk-rs/safe-cex-venue-asterdex/src/public_subscriptions.rs:283-324". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-87"></a>

### asterdex 87. `AsterDEXPublicWebsocket.connectAndSubscribe` — finding / explanation

The note accurately names asynchronous native dial and socket Error logging versus direct source onClose. However no reason is given for the error-event routing change: avoiding shared trait change alone does not explain why mirroring source would be worse. Audit the resulting socket close/reconnect effects before calling a new behavior defect.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts:70-91". Native: "sdk-rs/safe-cex-venue-asterdex/src/public_stream.rs:159-173; sdk-rs/safe-cex-exchange/src/websocket.rs:1065-1066". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-88"></a>

### asterdex 88. `AsterDEXPublicWebsocket.dispose` — supported / None

Cancellation of source surviving local kline waits/open IIFE is explicitly chosen with ownership reason; actual dispose drains native waits and open tasks.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts:595-607". Native: "sdk-rs/safe-cex-venue-asterdex/src/public_subscriptions.rs:441-469". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-89"></a>

### asterdex 89. `AsterDEXPublicWebsocket.listenOrderBook` — supported / None

Rc callback and fallible cleanup representation/lifetime costs are explicit and reasoned. Listener ids, stream reuse and cleanup lookup remain real.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts:466-487". Native: "sdk-rs/safe-cex-venue-asterdex/src/public_subscriptions.rs:206-246". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-90"></a>

### asterdex 90. `AsterDEXPublicWebsocket.onMessage` — finding / explanation

Actual JSON parser and onMessage route a ticker whose symbol is materialized D800 through actual handleTickerUpdate: its keyed Store target changes last to7 and ticker event exposes the D800 key before serialization. Native Value/String frame ingress cannot retain it, refusing the entire frame, not just a symbol field. Owned snapshot choice is separate; the blanket CHOSEN string clause does not price this program observation under section19.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts:204–264,317–333". Native: "sdk-rs/safe-cex-venue-asterdex/src/public_stream.rs:175–242,344–365". Source profile: `aster-public-message-consumer`. Policy: [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [anchor](../../MANIFEST-SPEC.md#closed-typed-output-boundary).

<a id="asterdex-91"></a>

### asterdex 91. `AsterDEXPublicWebsocket.onOpen` — supported / priced scheduling

WITHDRAWN proposed new scheduling finding: the frozen note explicitly prices replay/finally in one native poll versus the source additional await/microtask, and names native owner-scheduler cancellation rationale. Actual onOpen/subscribe source timing is measured, but does not turn this already declared cost into a new unpriced gap. Support is narrow, not whole-row conformance.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts:96-148". Native: "sdk-rs/safe-cex-venue-asterdex/src/public_subscriptions.rs:170-203". Source profile: `aster-onOpen-turn`. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="asterdex-92"></a>

### asterdex 92. `AsterDEXPublicWebsocket.subscribeToKline` — supported / None

Typed callbacks/queued captured names and owner-managed timer disposal are precise, reasoned costs. Source unexecuted queue and continued wait after unsubscribe are retained; no generic arbitrary-function argument used to manufacture a finding.

Source: "packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts:539-584". Native: "sdk-rs/safe-cex-venue-asterdex/src/public_subscriptions.rs:337-392". Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

## Gates and exclusions

Exact static-gate input and measured exits/log hashes are recorded in `gates.json` after annotation. The integrity gate compares all frozen identities/notes, pinned venue census and primary types, zero status/target/file-target changes, and published source-control hashes. Deliberate status and primary-type faults require named failures and restored passes. Extractor writer reconciles existing manifests; no manifest regeneration is used.

Not run for this audit: Cargo check/test/build, native mutation controls, workspace build, host clippy, wasm build/clippy, full TypeScript checking, monorepo suites, global parity readers, CI or live/authenticated exchange calls. Prior audits' runtime/build receipts remain attached to their original SHAs and do not cover this branch. Only source oracle and metadata/extractor/syntax gates are measured here.

At static input `7cdfc3f110`, writer0, metadata0 / named status fault1 / restored0, named primary-type fault1 / restored0, all three Node syntax checks0, extractor-check0 and diff-check0. Writer/check report zero updates/additions/orphans. Full SHA and log hashes are in `gates.json`. The later row90 citation-only correction does not relabel these measurements or rerun source controls.

All three local adversarial row-review scopes returned MERGE at `7cdfc3f110`, with one ticker-citation correction retained in `review-challenges.json`. Final citation/receipt delta review returned MERGE at `a112a778a3`, verifying all11 static log hashes and named failures; reviewers executed no gates. Root post-citation metadata and diff checks both exited0 at that SHA, recorded separately in `post-citation-gates.json`. Review and receipt-only descendants do not expand native/runtime coverage.
