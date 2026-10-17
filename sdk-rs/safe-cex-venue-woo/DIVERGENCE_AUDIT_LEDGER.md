# WOO lane divergence audit ledger

This is the cumulative identity ledger for this lane's divergent-backlog audits,
starting with the signer sample recorded in DIVERGENCE_AUDIT_EVIDENCE.md.
The latest enumeration stops at WooV3AlgoPlaceResponseRow.
It includes 80 enumerated cohort rows and one partially audited owning-class
row: **81 unique rows, 53 choice-only, 6 needing behavior code, and 22 MIXED
representation/interface rows without a new behavior-code repair**.
Of those six, three are repaired and three remain partial/open. The latter are
signV1, signV3 and WOOXExchange. The owner row is a partial audit, not proof of
complete member assembly. Two divergent-to-ported moves are counted separately.

C denotes the established choice-only outcome, R a source-contract defect repaired, and O
behavior code partly repaired with work still open. M denotes a measured MIXED
representation/interface cost, with no behavior-code repair in this audit.
These are audit-outcome/discovery labels,
not replacement manifest statuses; a repaired row may retain CHOSEN costs.
The native out-of-domain BAD-status panic and test-instrument fixes do not add
source-defect-bearing rows. Test code added to observe choices is not a behavior
repair. Canonical classification remains
`sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy`.

H = historical signer sample (9); S = symbol/numeric-response batch (8);
B = partial owning-class binding audit (1); Rg = regular-write batch (8);
A = algo-response batch (8); Ac = account batch (8); G = error/validator guards (8);
P = position evidence (8 new identities plus one previously counted re-audit);
Aq = algo request/target cohort (8); Ag = algo guards/metadata (8); Ad = algo input/result declarations (8).
normalizeSymbol occurs in H and S and is counted once:
9 + 8 - 1 + 1 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8 = 81.
The binary CHOSEN-versus-code counter does not exhaust the audit after G:
53 + 6 + 22 = 81. M is not silently counted as choice-only.
P adds five C and three M, then corrects historical WooV3CompletePositionModeEvidence
from C to M after its materialized-ID witness. That correction adds no identity.
Aq adds five C and three M, with no re-audits and no new behavior-code finding.
Ag adds four C and four M; the already-ported trigger predicate is not counted.
Ad adds one C and seven M, with no behavior-code discovery.
The latest marginal code rate is 0/8; Ag and Ad together are 0/16,
and P, Aq, Ag and Ad together are 0/32 across four consecutive cohorts.
The cumulative rate is 6/81 (about 7.4%). These coherent cohorts are not a
random sample, so this falling observed rate alone does not prove fleet-wide convergence.
The two class-binding member re-audits add no identities. Identically named
helpers in distinct source manifests remain distinct rows.

Every manifest below is under sdk-rs/parity/manifest/exchanges/woo/.

| Manifest | Kind | TS symbol | Cohort | Discovery |
| --- | --- | --- | --- | --- |
| woo.utils.ts.toml | const-fn | normalizeSymbol | H,S | C |
| woo.utils.ts.toml | const-fn | reverseSymbol | S | C |
| woo.api.ts.toml | const-fn | signV1 | H | O |
| woo.api.ts.toml | const-fn | signV3 | H | O |
| woo.v3.regular-writes.ts.toml | type | WooV3RegularWriteTarget | H | C |
| woo.v3.account-writes.ts.toml | type | WooV3CompletePositionModeEvidence | H,P | M |
| woo.v3.public-parity.ts.toml | type | WooV3PublicParityMismatch | H | C |
| woo.v3.runtime-selector.ts.toml | type | WooV3InvalidModeSelection | H | C |
| woo.v3.ticker.ts.toml | type | WooV3AuthoritativeTickerInfo | H | C |
| woo.ws-public.ts.toml | field-fn | WooPublicWebsocket.handleTickersStreamEvents | H | C |
| woo.exchange.ts.toml | class | WOOXExchange | B | O |
| woo.exchange.ts.toml | field-fn | WOOXExchange#parseFiniteFundingNumber | S | C |
| woo.exchange.ts.toml | field-fn | WOOXExchange#normalizeTimestampSeconds | S | C |
| woo.exchange.ts.toml | field-fn | WOOXExchange#normalizeTimestampMilliseconds | S | C |
| woo.exchange.ts.toml | field-fn | WOOXExchange#normalizeTransactionSymbol | S | C |
| woo.exchange.ts.toml | field-fn | WOOXExchange#normalizeRequestedSymbol | S | R |
| woo.exchange.ts.toml | field-fn | WOOXExchange#getTransactionHistorySymbolCandidates | S | C |
| woo.v3.regular-writes.ts.toml | const-fn | mapPlaceType | Rg | C |
| woo.v3.regular-writes.ts.toml | const-fn | buildWooV3RegularPlaceRequest | Rg | C |
| woo.v3.regular-writes.ts.toml | const-fn | buildWooV3RegularAmendRequest | Rg | C |
| woo.v3.regular-writes.ts.toml | const-fn | buildWooV3RegularCancelRequest | Rg | C |
| woo.v3.regular-writes.ts.toml | const-fn | buildWooV3RegularCancelPendingRequest | Rg | C |
| woo.v3.regular-writes.ts.toml | const-fn | decodeSuccessData | Rg | C |
| woo.v3.regular-writes.ts.toml | const-fn | decodeWooV3RegularPlaceResponse | Rg | R |
| woo.v3.regular-writes.ts.toml | const-fn | decodeMutationResponse | Rg | C |
| woo.v3.algo-writes.ts.toml | class | WooV3AlgoWriteResponseError | A | C |
| woo.v3.algo-writes.ts.toml | const-fn | decodeSuccessData | A | C |
| woo.v3.algo-writes.ts.toml | const-fn | decodePlaceRow | A | C |
| woo.v3.algo-writes.ts.toml | const-fn | decodeWooV3AlgoPlaceResponse | A | R |
| woo.v3.algo-writes.ts.toml | const-fn | decodeMutation | A | C |
| woo.v3.algo-writes.ts.toml | const-fn | decodeWooV3AlgoAmendResponse | A | C |
| woo.v3.algo-writes.ts.toml | const-fn | decodeWooV3AlgoCancelResponse | A | C |
| woo.v3.algo-writes.ts.toml | const-fn | decodeWooV3AlgoCancelPendingResponse | A | C |
| woo.v3.account-writes.ts.toml | const-fn | validateAccountProvenance | Ac | C |
| woo.v3.account-writes.ts.toml | const-fn | buildWooV3GetLeverageRequest | Ac | C |
| woo.v3.account-writes.ts.toml | const-fn | buildWooV3SetLeverageRequest | Ac | C |
| woo.v3.account-writes.ts.toml | class | WooV3AccountConfigResponseError | Ac | C |
| woo.v3.account-writes.ts.toml | const-fn | decodeSuccess | Ac | C |
| woo.v3.account-writes.ts.toml | const-fn | decodeWooV3GetLeverageResponse | Ac | C |
| woo.v3.account-writes.ts.toml | const-fn | decodeWooV3SetLeverageResponse | Ac | C |
| woo.v3.account-writes.ts.toml | const-fn | decodeWooV3SetPositionModeResponse | Ac | C |
| woo.exchange.ts.toml | const-fn | isWooRateLimitError | G | C |
| woo.exchange.ts.toml | const-fn | isWooPositionModeAlreadySetError | G | C |
| woo.v3.account-writes.ts.toml | const-fn | requireRecord | G | M |
| woo.v3.account-writes.ts.toml | const-fn | requireBoolean | G | C |
| woo.v3.account-writes.ts.toml | const-fn | requireInteger | G | C |
| woo.v3.account-writes.ts.toml | const-fn | requireString | G | M |
| woo.v3.account-writes.ts.toml | const-fn | requireEnum | G | M |
| woo.v3.account-writes.ts.toml | const-fn | requireExactKeys | G | M |
| woo.v3.account-writes.ts.toml | type | WooV3SetPositionModeInput | P | M |
| woo.v3.account-writes.ts.toml | type | WooV3SetPositionModeBuildResult | P | C |
| woo.v3.account-writes.ts.toml | const | WOO_V3_ACCOUNT_WRITE_PARITY | P | C |
| woo.v3.account-writes.ts.toml | const-fn | requirePerpetualSymbol | P | C |
| woo.v3.account-writes.ts.toml | const-fn | validateSnapshotEvidence | P | M |
| woo.v3.account-writes.ts.toml | const-fn | isExactDecimalZero | P | C |
| woo.v3.account-writes.ts.toml | const-fn | positionProvesNoExposure | P | C |
| woo.v3.account-writes.ts.toml | const-fn | buildWooV3SetPositionModeRequest | P | M |
| woo.v3.algo-writes.ts.toml | const-fn | requireDecimal | Aq | C |
| woo.v3.algo-writes.ts.toml | const-fn | validateTarget | Aq | C |
| woo.v3.algo-writes.ts.toml | const-fn | validateCommon | Aq | M |
| woo.v3.algo-writes.ts.toml | const-fn | buildWooV3AlgoPlaceRequest | Aq | M |
| woo.v3.algo-writes.ts.toml | const-fn | targetMatchesOrder | Aq | C |
| woo.v3.algo-writes.ts.toml | const-fn | buildWooV3AlgoAmendRequest | Aq | M |
| woo.v3.algo-writes.ts.toml | const-fn | buildWooV3AlgoCancelRequest | Aq | C |
| woo.v3.algo-writes.ts.toml | const-fn | buildWooV3AlgoCancelPendingRequest | Aq | C |
| woo.v3.algo-writes.ts.toml | const | WOO_V3_ALGO_WRITE_PARITY | Ag | C |
| woo.v3.algo-writes.ts.toml | const-fn | requireRecord | Ag | M |
| woo.v3.algo-writes.ts.toml | const-fn | requireArray | Ag | M |
| woo.v3.algo-writes.ts.toml | const-fn | requireBoolean | Ag | C |
| woo.v3.algo-writes.ts.toml | const-fn | requireInteger | Ag | C |
| woo.v3.algo-writes.ts.toml | const-fn | requireString | Ag | M |
| woo.v3.algo-writes.ts.toml | const-fn | requireEnum | Ag | M |
| woo.v3.algo-writes.ts.toml | const-fn | requirePerpetualSymbol | Ag | C |
| woo.v3.algo-writes.ts.toml | type | WooV3AlgoWriteTarget | Ad | C |
| woo.v3.algo-writes.ts.toml | type | WooV3StopMarketPlaceInput | Ad | M |
| woo.v3.algo-writes.ts.toml | type | WooV3TrailingPlaceInput | Ad | M |
| woo.v3.algo-writes.ts.toml | type | WooV3PositionalTriggerInput | Ad | M |
| woo.v3.algo-writes.ts.toml | type | WooV3PositionalPlaceInput | Ad | M |
| woo.v3.algo-writes.ts.toml | type | WooV3AlgoPlaceInput | Ad | M |
| woo.v3.algo-writes.ts.toml | type | WooV3AlgoPlaceBuildResult | Ad | M |
| woo.v3.algo-writes.ts.toml | type | WooV3AlgoPlaceResponseRow | Ad | M |

The earlier 16/14/2 and 24/21/3 figures in the historical evidence were explicitly
limited cohort counters, not this lane-wide total. Adding the signer sample
without deduplication would double-count normalizeSymbol. Omitting the owner
would hide a partially inspected, still-open code requirement.

Older WOO notification, mapper-timing, table-lookup, numeric, field-identity and
cardinality audits were inherited at the lane handoff and are not claimed here.
GATE_EVIDENCE.md, STORE_NOTIFICATION_AUDIT.md and TABLE_LOOKUP_AUDIT.md already
existed at the first signer repair 11673274; their relevant changes preceded
this lane's divergence receipt, first added in 58981a593a. Shared Git authorship
alone cannot identify agents; chronology and the explicit lane assignment set
this attribution boundary. Pending-row porting is not added to this audit of
already-divergent rows, and incidental dependency reads do not certify extra
manifest rows. All unlisted rows remain outside this finite audit claim.

## Production terminal-report follow-up

The live `mapLimitOrder` binding previously called the closed typed facade, so
IOC/FOK's missing ORDER_TYPE hit and MARKET's `0 || undefined` price threw before
terminal status/fill handling. The runtime mapper now preserves the actual source
values, including own undefined fields and the original info referent. Real
private-socket regressions for IOC, FOK and zero-price MARKET compile and fail
before the repair, then emit the exact fill and close the existing order after
it. A maintained old-binding control reproduces all three failures and restores
the stream suite. The source oracle executes actual TypeScript method bodies.
This is one production defect family, not three new discovery identities; the
separate typed facade retains its documented representation boundary.

NEW/PARTIAL inserts and snapshots still cross the shared typed Order boundary.
Three independent actual-host diagnostics reproduce NEW IOC, NEW zero-price
MARKET and PARTIAL IOC failures there; the shared-type owner has those fixtures.
No missing type is replaced with Market, and no undefined price becomes zero or
NaN in this repair. String/null quantity and other copied dynamic fields remain
separate measured obligations.
