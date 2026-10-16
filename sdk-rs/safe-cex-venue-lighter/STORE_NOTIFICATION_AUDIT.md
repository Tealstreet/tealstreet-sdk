# Store notification reachability audit

Audited Lighter on `c37b79d440` plus integration `aa632b5578`. This supersedes
the narrower direct-write census in GATE_EVIDENCE.md. Method reference: Bybit's
committed STORE_NOTIFICATION_AUDIT.md and actual-source reconnect witness.

## Scope and result

All 725 assigned rows were classified: venue 336 and SDK 389. The 192 ported
rows contain **94 callable declarations** (32 const-fn, 25 function, 37 field-fn)
and 98 data/type/enum declarations. A TypeScript AST walk resolved all 94 from
the manifests' source files, with zero unresolved declarations. The inventory
below records their direct calls; those calls were then followed manually to
local helpers, imported helpers and inherited core operations.

**Zero ported rows have a fixed internal path to a Store notification.** This
is a reachability result, not a grep result. In particular:

- None of the 94 calls send, reconnect, onClose, processClose, health publication,
  snapshot orchestration, or a notifying Store operation. The already claimed
  socket members are policy/time/read helpers, not send/connect lifecycle rows.
- `getWatchdogStaleThresholdMs` delegates only to the local threshold getter.
  Private rate/freshness and public cooldown reads terminate at
  `virtualClock.getCurrentTimeMs`, whose body is Date.now plus the stored offset.
- `hasHydratedTickerPrice` reads `parent.store.tickers` and uses Array.some and
  Number.isFinite. It never mutates Store or invokes a Store/core operation.
- `getInitialTickDelay` reaches inherited `BaseExchange.getTickInterval`, then
  `getExchangeMetadata` (table read) and `getTickIntervalMultiplier` (Map.get).
  Auth-failure scaling is arithmetic, not health publication.
- Deployment URL construction reaches `withPublicCorsProxy` and getTransport.
  Built-in provider publicCorsProxy callbacks return undefined or the configured
  public proxy constant; neither enters Store. Custom host callbacks may have
  arbitrary effects, but this audit does not equate those with a fixed internal
  notification path, matching Bybit's diagnostic-callback scope.
- Precision, address, integer-string and minimum helpers include calls into
  already-divergent local helpers. Their transitive bodies perform coercion,
  regex/string/numeric operations only. No presumed-pure helper was excluded
  merely because its own row was divergent or pending.
- The other local-helper chains (deployment/profile, builder fees/approval,
  margin/price candidates, partner rejection, close classification/backoff)
  terminate in table reads, coercions, string or numeric operations.

All 14 divergent callable rows were also inspected: nine coercion/string
helpers, four response flattening/parsing helpers and the market-reference-price
reader. They add no fixed notification path. Their existing independent
representation findings remain unchanged.

The earlier source scan covered nine direct update/setSetting calls across four
pending rows. The cutover refresh below broadens that inventory to every Store
mutation spelling; the historical nine is not a complete mutation inventory.

This is a point-in-time source audit, not a compiler-proven call graph or a CI
reachability gate. Dynamic dispatch and arbitrary user getters/callbacks are not
claimed effect-free. The earlier zero was correct but insufficiently scoped;
this explicit edge walk is the evidence for retaining it.

## Ported callable inventory

- `lighter.utils.ts::generateLighterClientId` → Date.now, Math.random().toString().slice, Math.random().toString, Math.random
- `lighter.utils.ts::bigIntToString` → value.toString
- `lighter.utils.ts::readNumber` → Number, Number.isFinite
- `lighter.utils.ts::decimalsFromScale` → Number.isFinite, Math.log10, Number.isInteger
- `lighter.utils.ts::resolveLighterCancelTargets` → orders.flatMap, Number.parseInt, Number.isFinite
- `lighter.utils.ts::clampDecimals` → Number.isFinite, Math.max, Math.min, Math.trunc
- `lighter.utils.ts::getLighterAmountDecimals` → readNumber, clampDecimals, decimalsFromScale, getPrecision
- `lighter.utils.ts::getLighterPriceDecimals` → readNumber, clampDecimals, decimalsFromScale, getPrecision
- `lighter.utils.ts::getLighterOrderMinimums` → normalizeNonNegativeNumber
- `lighter.utils.ts::toLighterScaledInt` → Number.isFinite, Error, clampDecimals, BigInt, Math.round
- `lighter.ws-public-parsers.ts::shouldDispatchLighterPublicMessage` → Set, topic.replace, Array.from(channelVariants).some, Array.from, data.includes
- `lighter.ws-public-parsers.ts::parseLighterWsNumber` → Number.isFinite, parseFloat
- `lighter.ws-public-parsers.ts::parseLighterWsBookSidePrice` → Array.isArray, parseLighterWsNumber
- `lighter.ws-public-parsers.ts::extractLighterTickerMarketIdFromChannel` → String(channel || '').match, String
- `lighter.ws-private.ts::LighterPrivateWebsocket.shouldEnforcePongMissReconnect` → (no calls)
- `lighter.ws-private.ts::LighterPrivateWebsocket.isReadonlyFallbackActive` → (no calls)
- `lighter.ws-private.ts::LighterPrivateWebsocket#shouldUseRestOrderReconciliation` → (no calls)
- `lighter.ws-private.ts::LighterPrivateWebsocket#getPingIntervalMs` → (no calls)
- `lighter.ws-private.ts::LighterPrivateWebsocket#getIdlePingThresholdMs` → (no calls)
- `lighter.ws-private.ts::LighterPrivateWebsocket#getIdleReconnectThresholdMs` → (no calls)
- `lighter.ws-private.ts::LighterPrivateWebsocket.getWatchdogStaleThresholdMs` → this.getIdleReconnectThresholdMs
- `lighter.ws-private.ts::LighterPrivateWebsocket#isWsRateLimited` → virtualClock.getCurrentTimeMs
- `lighter.ws-private.ts::LighterPrivateWebsocket#shouldForceToastPrivateStreamRateLimit` → (no calls)
- `lighter.ws-private.ts::LighterPrivateWebsocket#isRecentStreamEvent` → Number.isFinite, Date.now, virtualClock.getCurrentTimeMs
- `lighter.signer.ts::getProvider` → (no calls)
- `lighter.signer.ts::isLighterInvalidNonceError` → String, String(record?.message ?? error ?? '').toLowerCase, message.includes
- `lighter.builder-fee.ts::resolveMode` → (no calls)
- `lighter.builder-fee.ts::parsePositiveIntegerBigInt` → value.trim, /^\d+$/.test, BigInt
- `lighter.builder-fee.ts::getDefaultIntegratorAccountIndex` → isLighterRobinhoodExchange
- `lighter.builder-fee.ts::resolveLighterBuilderFeeConfig` → resolveMode, getDefaultIntegratorAccountIndex, parsePositiveIntegerBigInt
- `lighter.ws-public.ts::normalizeLighterFundingRate` → (no calls)
- `lighter.ws-public.ts::LighterPublicWebsocket.shouldEnforcePongMissReconnect` → (no calls)
- `lighter.ws-public.ts::LighterPublicWebsocket.getWatchdogStaleThresholdMs` → (no calls)
- `lighter.ws-public.ts::LighterPublicWebsocket#isTickerRestFallbackCoolingDown` → virtualClock.getCurrentTimeMs
- `lighter.ws-public.ts::LighterPublicWebsocket#hasHydratedTickerPrice` → this.parent.store.tickers.some, [ticker.mark, ticker.last, ticker.index, ticker.bid, ticker.ask].some, Number.isFinite
- `lighter.builder-approval.ts::getBaseUrl` → overrideBaseUrl.trim, overrideBaseUrl.trim().replace, getLighterApiBaseUrl
- `lighter.builder-approval.ts::toBigInt` → BigInt, String
- `lighter.builder-approval.ts::toNumber` → Number, Number.isFinite
- `lighter.builder-approval.ts::toHex10` → BigInt, Error, normalized.toString(16).padStart, normalized.toString
- `lighter.builder-approval.ts::approvalFeeCapsHaveNonZeroFees` → hasNonZeroLighterIntegratorFee
- `lighter.builder-approval.ts::getDefaultIntegratorAccountIndex` → isLighterRobinhoodExchange, parseLighterPositiveIntegerString, Error, BigInt
- `lighter.builder-approval.ts::buildLighterApproveIntegratorL1Message` → toHex10
- `lighter.builder-approval.ts::createLighterL1SignatureHelperUrl` → URLSearchParams, params.nonce.toString, params.accountIndex.toString, String, params.integratorAccountIndex.toString, query.toString
- `lighter.types.ts::isLighterRobinhoodExchange` → (no calls)
- `lighter.types.ts::resolveLighterDeploymentNetwork` → isLighterRobinhoodExchange
- `lighter.types.ts::resolveLighterDeploymentProfile` → resolveLighterDeploymentNetwork
- `lighter.types.ts::getBaseUrl` → withPublicCorsProxy
- `lighter.types.ts::getLighterApiBaseUrl` → getBaseUrl, resolveLighterDeploymentNetwork
- `lighter.types.ts::getLighterWsUrl` → resolveLighterDeploymentProfile
- `lighter.types.ts::resolveLighterInterval` → Error
- `lighter.ws-public-ohlcv.ts::buildLighterCandleTopic` → (no calls)
- `lighter.ws-public-ohlcv.ts::buildLighterTradesTopic` → (no calls)
- `lighter.ws-public-ohlcv.ts::shouldAttemptLighterOhlcvRecovery` → (no calls)
- `lighter.ws-public-ohlcv.ts::shouldPromoteLighterOhlcvRecoveryToProxy` → (no calls)
- `lighter.account-tier.ts::normalizeLighterAccountTier` → String(value ?? '')     .trim()     .toLowerCase, String(value ?? '')     .trim, String
- `lighter.account-tier.ts::asRecord` → (no calls)
- `lighter.account-tier.ts::extractLighterAccountTierFromPayload` → asRecord, normalizeLighterAccountTier
- `lighter.account-tier.ts::isLighterAccountTierEligibleForPartnerAttribution` → (no calls)
- `lighter.account-tier.ts::hasNonZeroLighterIntegratorFee` → Number, Number.isFinite
- `lighter.account-tier.ts::isLighterPartnerAttributionTierRejection` → String(params.code ?? '').trim, String, String(params.message ?? '').toLowerCase, message.includes
- `lighter.account-tier.ts::isLighterPartnerAttributionApprovalMissingRejection` → String(params.code ?? '').trim, String, String(params.message ?? '').toLowerCase, message.includes
- `lighter.exchange.ts::parseLighterMarketSymbol` → symbol.includes, symbol.split, LIGHTER_KNOWN_QUOTES.find, symbol.endsWith, symbol.slice
- `lighter.exchange.ts::isLighterPerpMarket` → String(market.market_type ?? market.marketType ?? '').toLowerCase, String
- `lighter.exchange.ts::getLighterMarketLeverageMax` → Number.parseFloat, String, Number.isFinite
- `lighter.exchange.ts::parseLighterMarginFractionRatio` → Number.parseFloat, String, Number.isFinite
- `lighter.exchange.ts::normalizeLighterStopSlippagePct` → Number.parseFloat, String(value ?? '').trim, String, Number.isFinite, Math.min
- `lighter.exchange.ts::LighterExchange#parseAccountIndex` → BigInt, String
- `lighter.exchange.ts::LighterExchange#inferStopLossTypeFromReduceOnlyLimit` → Number.isFinite, this.getPositivePriceCandidate
- `lighter.exchange.ts::LighterExchange#extractL1AddressCandidate` → value.trim, isLighterL1Address, trimmed.match
- `lighter.exchange.ts::LighterExchange#isWasmRuntimeIssueMessage` → message.toLowerCase, normalized.includes
- `lighter.exchange.ts::LighterExchange#getInitialTickDelay` → this.getTickInterval, Math.min
- `lighter.exchange.ts::LighterExchange#parseMarginMode` → String(value).toLowerCase, String
- `lighter.exchange.ts::LighterExchange#marginModeToMarginType` → (no calls)
- `lighter.exchange.ts::LighterExchange#marginTypeToMarginMode` → (no calls)
- `lighter.exchange.ts::LighterExchange#getMarketConfiguredMarginMode` → this.parseMarginMode
- `lighter.exchange.ts::LighterExchange#getOhlcvFallbackKey` → (no calls)
- `lighter.exchange.ts::LighterExchange#getReduceOnlyClosingPositionSide` → (no calls)
- `lighter.exchange.ts::LighterExchange#hasOpenReduceOnlyTargetPosition` → this.getReduceOnlyClosingPositionSide, positions.some
- `lighter.exchange.ts::LighterExchange#buildNoPositionReduceOnlyMessage` → (no calls)
- `lighter.exchange.ts::LighterExchange#isRateLimitOrderError` → Number, String(record?.code ?? record?.response?.data?.code ?? '').toLowerCase, String, message.toLowerCase, normalizedMessage.includes
- `lighter.exchange.ts::LighterExchange#isInvalidMarginModeOrderError` → String(record?.code ?? record?.response?.data?.code ?? '').toLowerCase, String, message.toLowerCase, normalizedMessage.includes
- `lighter.exchange.ts::LighterExchange#isPartnerAttributionTierOrderError` → isLighterPartnerAttributionTierRejection
- `lighter.exchange.ts::LighterExchange#isPartnerAttributionApprovalMissingOrderError` → isLighterPartnerAttributionApprovalMissingRejection
- `lighter.exchange.ts::LighterExchange#buildInvalidMarginModeOrderMessage` → this.getMarketConfiguredMarginMode
- `lighter.exchange.ts::LighterExchange#getOrderMinimumViolationReasons` → [       isBelowBaseMinimum ? \`base ${amount} < min_base_amount ${minBaseAmount}\` : undefined,       isBelowQuoteMinimum ? \`quote ${quoteAmount} < min_quote_amount ${minQuoteAmount}\` : undefined,     ]       .filter(Boolean)       .join, [       isBelowBaseMinimum ? \`base ${amount} < min_base_amount ${minBaseAmount}\` : undefined,       isBelowQuoteMinimum ? \`quote ${quoteAmount} < min_quote_amount ${minQuoteAmount}\` : undefined,     ]       .filter
- `lighter.exchange.ts::LighterExchange#getPositivePriceCandidate` → values.find, Number.isFinite
- `lighter.exchange.ts::LighterExchange#resolveStopSlippagePct` → normalizeLighterStopSlippagePct
- `lighter.exchange.ts::LighterExchange#shouldUseBatchedCreateOrders` → orders.every, Number.isFinite
- `lighter.exchange.ts::LighterExchange#snapToLeverageTier` → Math.abs
- `lighter.ws-policy.ts::calculateExponentialBackoffDelay` → Math.min, Math.pow
- `lighter.ws-policy.ts::classifyLighterPublicClose` → String(closeReason || '').toLowerCase, String, normalizedReason.includes, LIGHTER_POLICY_REASON_RE.test
- `lighter.ws-policy.ts::calculateLighterPublicReconnectDelay` → calculateExponentialBackoffDelay, Math.max
- `lighter.ws-policy.ts::classifyLighterPrivateClose` → String, normalizedReason.toLowerCase, LIGHTER_AUTH_REASON_RE.test, LIGHTER_POLICY_REASON_RE.test, lowerReason.includes
- `lighter.ws-policy.ts::calculateLighterPrivateReconnectDelay` → calculateExponentialBackoffDelay, Math.max

## Store cutover refresh at integration 9419b340d8a6

The current 190 ported rows still contain the same 94 callable declarations;
58 SDK ported rows are constants/types, not runtime SDK clients. All 14 divergent
callables remain outside fixed notification paths. No Rust production module or
test imports Store, invokes a Store mutation or converts StoreMutationError.
The previous transitive inventory still applies: no new callable row was claimed,
local helper code is unchanged, and the incoming Lighter source delta adds only
recoversAfterOffline=false fields to pending socket classes. Host callbacks retain
the explicit exclusion above. This is a manual reachability audit, not a compiler
call graph.

A single TypeScript AST process enumerated every direct *.store.<method>() call
in the three runtime source files. All 25 calls belong to 14 pending rows:

| File / owning row | Lines and mutations |
| --- | --- |
| lighter.exchange.ts / changeAccountTier | 728 update |
| lighter.exchange.ts / start | 950 setSetting; 968,1004,1023,1032 update; 1022 addOrUpdatePositionsFull |
| lighter.exchange.ts / fetchAndSetPrivateSnapshotHelper | 1070 addOrUpdatePositionsFull; 1071 addOrUpdateOrdersFull; 1073,1080 update |
| lighter.exchange.ts / fetchAndSetBalance | 1112 update |
| lighter.exchange.ts / refreshMarginModeStateAfterInvalidOrder | 2209 addOrUpdatePositionsFull |
| lighter.exchange.ts / assertReduceOnlyLimitHasPosition | 2289 addOrUpdatePositionsFull |
| lighter.exchange.ts / cancelOrders | 2774 removeOrders |
| lighter.exchange.ts / syncLeverageFromExchange | 2933 addOrUpdatePositionsFull |
| lighter.ws-public.ts / handleTickerEvent | 1114 updateTicker; 1128 updatePosition |
| lighter.ws-public.ts / handleMarketStatsEvent | 1168 updateTicker |
| lighter.ws-private.ts / throttleFetchOrders | 492 addOrUpdateOrdersFull |
| lighter.ws-private.ts / throttleFetchPositions | 504 addOrUpdatePositionsFull |
| lighter.ws-private.ts / handleOrderTopic | 1206 removeOrders; 1209,1218 addOrUpdateOrders |
| lighter.ws-private.ts / handlePositionTopic | 1247 updatePositions |

Real TOML parsing confirms every owning row remains pending, and no venue/SDK
note cites an unresolved Store notification contract. Zero Rust sites require
migration, zero notification FINDINGs can be promoted, and zero additional Rust
sites need routing. All-target cargo check passes against the cutover.

Future implementations consume Result<(), StoreMutationError> and preserve the
Patch/Notification/InvalidOperation distinction. Notification propagation retains
the opaque subscriber id and cause; Store-local ids must not be deduplicated
across Stores. A source discard requires a named ts-bug site rather than a silent
Ok or inferred string conversion. The shared contract has landed; it is no longer
a reason to hold a writer or retain a notification-only FINDING.

Counts ported/divergent/pending: venue132/18/186, SDK58/63/268. Existing numeric,
UTF8, depth and legacy payload findings are independent and remain unchanged.

### Upstream identity limitation at the quoted cutover SHA

At 9419b340d8a6, StoreNotificationError retains subscriber_id, but
safe-cex-exchange/src/runtime.rs::ExchangeErrorMetadata has no store_subscriber_ids
field. Its From<StoreMutationError> conversion never reads subscriber_id: it
returns the original ExchangeError clone or projects message and response. The
announced inner-to-outer identity list is not present at this SHA. This was routed
to core; no local shim is added. The zero current Lighter conversion-site count
is unchanged. Future propagation must recheck the shared conversion before
claiming subscriber identity retention.

### Identity limitation closed at integration 9af82bfeae

PR 7248 is present in the fetched integration tree. ExchangeErrorMetadata now
carries store_subscriber_ids: Vec<u64>. Both recovered-original and fallback
branches of From<StoreMutationError> read notification.subscriber_id() and append
it after the inner metadata, retaining inner-to-outer order. IDs remain Store-local;
equal numbers from different Stores must not be deduplicated.

This closes the upstream limitation recorded above, not a Lighter runtime parity
row: there are still zero local Store conversions, zero identity-preserving boxes
to simplify and zero subscriber-ID manifest FINDINGs to promote. The 25 source
writes in 14 pending rows remain the adoption inventory. No local representation
or shim was introduced. Counts remain venue132/18/186 and SDK58/63/268.

### Enumeration after the MEXC repair at integration a09ea6489b

Iteration 1 after merging this base at d922626f79: `cargo check --locked
-p safe-cex-venue-lighter --all-targets --jobs 1` exited 0, with zero new
diagnostics or local mutation sites. The command used `sdk-rs/Cargo.toml`.
This checks Lighter and its dependencies, not KuCoin or the wasm workspace;
it makes no workspace compilation claim. KuCoin's six newly exposed unused
Results are outside this scoped result and remain owned by that lane.

The subsequent collection-audit test initially lacked its generic Serialize
bound. That was a test-only compilation error, repaired before its nine-case
baseline passed; it was not a Store cutover or production finding.
