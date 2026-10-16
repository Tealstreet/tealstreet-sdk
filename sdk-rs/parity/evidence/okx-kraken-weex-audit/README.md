# OKX, Kraken and WEEX original 36-row sample

The subsequent [full 92-row audit](FULL-AUDIT.md) extends this preserved sample.
The measurements and byte-identity statements below belong to the original
sample receipt 9c8d968ace, before the later finding-note annotations.

Frozen source/manifest input: `d74a1c01f77439f9df61fa330063be194aab7576`.
This branch starts directly at that integration commit, with no unreviewed topic
ancestor. Selection was committed and pushed at
`f13f938e0e8dba1d4744bde441586219bd5eef2d` before classification.
TypeScript, Rust production and all manifests remain byte-identical to that
input. **Zero status rows move.** Findings are recorded here; this audit does not
silently rewrite the explanations being evaluated.

The [frozen selection](selection.json) uses the prior
[divergence audit](../../DIVERGENCE_AUDIT.md)'s method: select divergent notes
containing CHOSEN, sort by SHA256 of UTF-8 `divergence-audit:` + source path +
`::` + TS symbol, take the first twelve per venue. No replacement after inspection.
The population is a census; the classifications are a finite sample with no
statistical population estimate. The three ineligible divergent rows remain
outside this sample, as do fifty-three eligible rows.

| Venue | Ported | Divergent | Pending | CHOSEN-containing eligible | Sample |
| --- | ---: | ---: | ---: | ---: | ---: |
| OKX | 137 | 26 | 0 | 24 | 12 |
| Kraken | 102 | 31 | 0 | 30 | 12 |
| WEEX | 121 | 35 | 0 | 35 | 12 |

Canonical declarations remain at [§14](../../MANIFEST-SPEC.md#shared-utf8-string-boundary),
[§15](../../MANIFEST-SPEC.md#closed-typed-output-boundary),
[§16](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[§17](../../MANIFEST-SPEC.md#numeric-ingress-defect),
[§18](../../MANIFEST-SPEC.md#divergence-taxonomy), and
[§19](../../MANIFEST-SPEC.md#forced-surrogate-conflict).
The observations below apply those declarations; they do not duplicate them.

“Supported” means the stated cost is supported within the inspected boundary,
not that all behavior of a symbol is equivalent. “Finding” names an omitted cost,
incorrect scope statement or an already explicit open defect. “Unresolved” means
the proposed wider claim was withdrawn for lack of evidence. A finding does not
imply that its solution is a production body change.

| Venue | Supported | Finding | Unresolved | Last iteration |
| --- | ---: | ---: | ---: | ---: |
| OKX | 6 | 6 | 0 | 12, OKXEUExchange |
| Kraken | 5 | 7 | 0 | 12, parseTriggerSignal |
| WEEX | 5 | 6 | 1 | 12, toIntegerString |
| Fixed sample | 16/36 | 19/36 | 1/36 | 36 selected rows |

Eighteen are newly recorded audit findings; OKX client-order lookup is an
**existing** FINDING at the frozen input, already addressed by the separate
PR #7914. Its repair is not copied into this audit and is not counted as new.
Fifty-six divergent rows are outside the sample. No global venue clearance or
exhaustive consumer, coercion, mutation or adapter audit is claimed.

## OKX, frozen iteration order

All paths in this table are relative to `packages/safe-cex/src/exchanges/okx/`
(TS) or `sdk-rs/safe-cex-venue-okx/src/` (Rust). Source probes execute actual
selected members with fixed explicit receivers, not complete SDK assembly.

| # | Symbol | Classification and traced observation |
| ---: | --- | --- |
| 1 | OKXPublicWebsocket.handleTickerEvents | **Justification finding.** `okx.ws-public.ts:227` forwards update.instId into actual Store key lookup; the new source probe updates the D800-keyed ticker's last to 7 and notifies once. `public_socket.rs:273` takes Value and converts ID to String before Store. The note prices array/diagnostic and routed prototype costs, but does not identify this programmatic unit and whole-frame ingress refusal under §19. |
| 2 | OKXPublicWebsocket.handleMarkPriceEvents | **Justification finding.** `okx.ws-public.ts:245` sends D800 to markPriceLoaded and Store; the actual Store body updates mark to 11. Native `public_socket.rs:228` takes Value. Price this unit/whole-frame refusal separately from array shape, parent callbacks and routed Store effects. |
| 3 | OKXExchange.findOrderByClientOrderId | **Existing behavior finding, not new.** `okx.exchange.ts:141,143` has two inherited table lookups; frozen `rest_operations.rs:223` substitutes live and retains a normal enum status. The note already calls this unresolved. §15 analysis belongs to its separate repair PR; this audit supplies no new lookup execution and no claim about the injected mapper's full SDK linkage. |
| 4 | OKXPrivateWebsocket#sumFinite | **Supported cost.** `okx.ws-private.ts:31` calls parseFinite through the receiver; `finite.rs:16` uses the native JSON helper. Source callable coercion returns 7.5 for the fixed object, while the note explicitly excludes executable hooks/sparse/cyclic inputs and explains the choice. Arbitrary receiver replacement is not proved. |
| 5 | OKXExchange#placeAlgoOrderBatch | **Supported output/refusal cost.** `okx.exchange.ts:2218` and `rest_operations.rs:77` show serial requests, truthy-ID handling and retained refusal after later requests/errors. The note explicitly prices non-string IDs and non-array diagnostics. The tentative immediate-eager-POST finding was withdrawn: actual imported mapSeries awaits each array element before callback invocation. This does not prove arbitrary scheduling/drop equivalence; no new native scheduling execution is claimed. |
| 6 | mapOkxHistoricalOrder | **Supported closed-output cost.** `okx.order-history.ts:87,88` inherits non-enum values; `order_history.rs:79` preserves TableHit and defers refusal until later callbacks/coercions complete. The note names constructor/__proto__, whole-record refusal and later effects. Native body and existing witnesses inspected; no new history corpus executed. |
| 7 | OKXExchange | **Supported representation cost.** The source class at `okx.exchange.ts:119` exposes own arrows; `family_lifecycle.rs:10` has explicit owned carriers. The note names replaceable methods/prototype identity and prices the chosen object model. Host field initialization/receiver assembly at `family_lifecycle.rs:70` remains an explicit seam, not proved concrete integration. |
| 8 | OKXPublicWebsocket.handleFundingRateEvents | **Justification finding.** `okx.ws-public.ts:262` sends D800 through actual Store lookup and writes rate 17 in the source probe. Native `public_socket.rs:211` requires Value/String. The array-input note omits the programmatic unit and larger whole-frame refusal; routed prototype behavior remains separately owned. |
| 9 | OKXPublicWebsocket.handleIndexTickerEvents | **Justification finding.** `okx.ws-public.ts:250` passes D800 to the real method's market selector and loaded callback; the fixed selector returns that ID, and the actual Store body writes index 13. Native `public_socket.rs:248` uses Value and borrowed str IDs. This is a program key, not human-only diagnostic text. Selector implementation/full class assembly are not claimed by this witness. |
| 10 | OKXPrivateWebsocket#parseFinite | **Supported cost.** `okx.ws-private.ts:26` accepts callable String coercion; `finite.rs:4` uses represented JSON inputs. The same live callable profile pins 7.5; the note explicitly prices that exclusion. No arbitrary executable-object parity or whole socket proof. |
| 11 | OKXExchange#formatMarketPrettySymbol | **Justification finding.** `okx.exchange.ts:223` returns units D800, slash, Q, space, XPERP in the new actual-source probe. `exchange.rs:215` returns Option<String>. The frozen note calls the surrogate ingress cost CHOSEN despite the inspectable returned unit; its independent dynamic-receiver restriction remains separately priced. The exact returned unit, input refusal and missing entire pretty symbol need §19 treatment. |
| 12 | OKXEUExchange | **Supported representation cost.** `okxeu.exchange.ts:8` inherits from the global class; `family_lifecycle.rs:18` owns a base carrier. The note names the missing JS prototype/dynamic-method surface and the composition choice. EU behavior and callback host implementation remain independent rows. |

The four handler unit observations compose the actual `DefaultStore.updateTicker`
and `stripTickerInfoFromChanges` bodies (`store/store.base.ts:306,922`) with a
fixed receiver containing one ticker. They demonstrate a key lookup, field write
and notification, without claiming constructor, transport/parser or complete
Store assembly coverage. Required native Value/String surfaces and ingress
refusal paths were inspected, not newly executed.

## Kraken, frozen iteration order

Paths below are relative to the Kraken source and crate directories.
The new source probes close the two producer-specific gaps identified by the
read-only reviewer for Spot positions and account-log page. Token witnesses were
not reused as evidence for either producer.

| # | Symbol | Classification and traced observation |
| ---: | --- | --- |
| 1 | createKrakenApiError | **Justification finding.** Actual `kraken.api.ts:40` returns Error.message holding D800; caller inspection precedes JSON. `helpers.rs:28` requires str and ExchangeError's String. The pure CHOSEN note/utility declaration prices snapshots, Error ABI and Display, but omits this returned message unit. No human-only consumer assumption or native runtime refusal message is invented. |
| 2 | KrakenSpotPublicWebsocket | **Supported assembly cost.** Source fixes its arrows (`krakenspot.ws-public.ts:16`); native `classes.rs:49,71` requires external member bindings. The note names first missing connectAndSubscribe and disclaims fixed production body adapters. Existing composition fixtures inspected, not newly executed. |
| 3 | KrakenSpotPrivateWebsocket | **Supported assembly cost.** Source fixed members at `krakenspot.ws-private.ts:17` versus external bindings at `classes.rs:706`; eight-arrow expectation appears independently in `tests/composition.rs:1565`. This supports the stated requirement, not complete production wiring. |
| 4 | KrakenSpotExchange | **Supported assembly cost.** Source construction at `krakenspot.exchange.ts:59` versus `classes.rs:120`/`construction.rs:42`; first missing start and seventeen arrows are explicit fixtures. The externally assembled class boundary is honestly disclosed. |
| 5 | mapKrakenAccountLog | **Justification finding.** Actual `kraken.income.ts:134` returns one selected record with tranId units D800 followed by :pnl. Native `income.rs:131` decodes an owned batch and returns String fields. The note should separately price the lost entire batch at ingress under §19, alongside its executable-object/depth choices. New source execution; native decoder/refusal code and existing utility tests inspected only. |
| 6 | KrakenExchange.fetchLeverageSettings | **Justification/scope finding.** Actual `kraken.exchange.ts:1428` stores D800 in its Map; the source probe reads that real key back as 7. Consumers call leverageSettings.get at source lines737/778. `reads.rs:161` uses captured JSON and cache callbacks. Pure CHOSEN does not name that program key or whole-response loss. “Constructors remain pending” is also stale at the frozen manifest: constructor is ported. No full constructor/cache-host linkage proof. |
| 7 | mapKrakenFill | **Justification finding.** `kraken.fills.ts:55` returns id units D800; native `fills.rs:31` decodes the record and requires Execution's String. Price the whole-input refusal separately under §19; chosen executable-object/depth restrictions remain. |
| 8 | KrakenSpotExchange.fetchPositions | **Justification/scope finding.** New actual `krakenspot.exchange.ts:610` execution returns two positions with D800 symbols and IDs D800:long / D800:short. `reads.rs:130` requires shared typed Market strings; there is no representable input for this source market, losing the positions result. The note's pure CHOSEN explanation omits the exact units and has stale constructor-pending wording. Direct-vector versus Promise adaptation is already priced and is not a new finding. |
| 9 | KrakenExchange.fetchOHLCV | **Entry-effect finding.** New actual-source execution observes clock then xhr.get before the caller awaits the returned Promise (`kraken.exchange.ts:1063,1102`). Native `futures_candles.rs:36,76,92` places both in an async fn body, so invocation without polling performs neither. Its referenced parsed-read boundary says read invocation occurs at entry, unlike this body. Existing tests immediately poll. The separate mixed-NaN rejection is already priced. Native delayed/drop behavior is a static observation, not a newly run native witness. |
| 10 | KrakenExchange#_fetchAccountLogPage | **Justification/scope finding.** New actual `kraken.exchange.ts:852,876` execution returns tranId D800:pnl from a one-row successful response. `reads.rs:51` decodes the whole response before mapping. Exact producer/whole-response loss needs §19 pricing; the note also falsely says constructors remain pending. Eager read and post-await Store resolution are supported by body inspection. |
| 11 | optionalNum | **Supported input cost.** Source presence/Number at `kraken.fills.ts:37`; native `fills.rs:24`. The utility declaration explicitly excludes executable hooks/BigInt. A source surrogate yielding absence does not prove a returned-unit conflict, so that proposed FORCED inference is withdrawn for this row. |
| 12 | parseTriggerSignal | **Supported input cost.** Source `kraken.exchange.ts:71` and native `helpers.rs:8` return the closed trigger enum/absence. The executable input boundary is explicit. Existing surrogate fixture returns absence; no returned surrogate is inferred. |

## WEEX, frozen iteration order

| # | Symbol | Classification and traced observation |
| ---: | --- | --- |
| 1 | normalizeOrderSide | **Supported input cost.** Source uppercase lookup `weex.utils.ts:159` and native `utils.rs:85`; used by order normalization at source182. Notes price callable/prototype/depth exclusions. No retained output unit is established by their numeric/enum-returning surrogate fixtures. |
| 2 | normalizeMarket | **Alias justification finding.** Source `weex.utils.ts:101` returns info: raw; native `market_data.rs:106` clones it. New source mutations show input.extra 1→7 changes output.info, and output.info 7→11 changes input. The note prices typed string projection but omits this independent §16 snapshot cost. Native snapshot direction follows clone inspection, not a newly executed Rust alias test. No new market-symbol unit probe is claimed. |
| 3 | normalizePercentage | **Supported input cost.** Source `weex.utils.ts:105` and native `utils.rs:150`; used by ticker percentage at source135. Executable/depth restrictions are stated. Numeric output does not itself retain an incoming surrogate. |
| 4 | normalizeTicker | **Surrogate and alias justification finding.** Actual source returns D800 at info.ticker.extra; native captured Value refuses the whole input, losing the entire ticker. Independent source mutations prove input↔output aliases for rawTicker and rawBook (values 7/11 and 13/17). Native `market_data.rs:133,137` clones both. The note omits §19's exact whole-result cost and §16's mutation directions. |
| 5 | WeexExchangeInfoResponse | **Justification finding.** Actual shared jsonParse returns assets[0].name with unit D800; the source Record<string,any> declaration (`weex.types.ts`) admits it, as its existing response typechecker fixture shows. Native `response_types.rs` uses captured JSON/UTF-8 values; whole-record refusal is larger than field loss. Pure CHOSEN must separate §19 from depth/projection choices. This is declared-shape/parser evidence, not Axios runtime validation. |
| 6 | normalizeOrderBook | **Supported input cost.** Source `weex.utils.ts:474` and native `utils.rs:293` retain represented normalization/totals. The note prices executable/deep and malformed iterable restrictions. Its surrogate example filters into numeric output; a FORCED returned-unit claim was withdrawn. Full imported-host parity is not inferred. |
| 7 | normalizeOrderStatus | **Supported input cost.** Source `weex.utils.ts:164`, native `utils.rs:96`; consumed at source196. Represented fallback and dynamic/depth exclusions are explicit; no exhaustive coercion claim. |
| 8 | fakePositionsForMarket | **Justification finding.** Actual `weex.utils.ts:484` returns two positions whose symbol retains D800. Shared native Market input and Position strings cannot carry that source market/result. Name the absent entire positions result under §19, independently of typed numeric/enum choices. No incoming info alias is claimed. |
| 9 | WeexPrivateWebsocket#throttledFetchPositions | **Unresolved wider timing claim; reporting cost supported.** Source empty catch (`weex.ws-private.ts:216`) versus native reporting (`private_feed.rs:399`) is visible and explicitly chosen. Native run_refresh_timer awaits a host future at421; the existing test's future always returns Pending first. Ready resolution, post-callback disposal and dropped observation are not established. The proposed scheduling defect is withdrawn; the row is not counted as a finding or full parity pass. |
| 10 | WeexPublicWebsocket#handleDepth | **Stale-scope finding only.** Source catch(() => {}) at `weex.ws-public.ts:317` versus native observer at `public_feed.rs:593` supports its reporting choice. But its note says the enclosing class remains pending; the frozen class row is divergent. No new reporting repair or observer lifecycle proof is claimed. |
| 11 | WeexPrivateWebsocket | **Supported assembly cost.** `classes.rs:164` initializes six fields/fourteen arrows and checks external bindings before super; source fields at `weex.ws-private.ts:15`. The note explicitly disclaims fixed production transport/Store/timer/body wiring. Existing tests and member inventories inspected only. |
| 12 | toIntegerString | **Justification finding.** Actual `weex.utils.ts:47` returns D800 unchanged, inspected before wire capture. Native `utils.rs:142` returns String. Source uses the result as lastUpdateId (`weex.exchange.ts:469`, `weex.ws-public.ts:335`), which is program data. Price the exact absent string under §19 separately from executable/depth choices. |

## Evidence, withdrawals and limits

[classification.json](classification.json) records the fixed per-row verdicts.
[Source observations](source-observations.mjs) execute twenty fixed profiles,
stopping at weex-response-unit. [The runner](run-controls.py) runs twenty actual
source mutations in memory, independently requiring the named assertion and
AssertionError, then fresh restored executions. No source file mutation or Rust
production mutation is performed. This is source discrimination, not native
implementation proof. Native paths listed above were inspected only.

At `3f2d53594f`, the all-profile source command exited **0**. All twenty source
controls returned **baseline 0 / mutant 1 / restored 0**; each failed its named
assertion, and tracked inputs remained unchanged. Node v24.20.0, arm64. See
[SHA-bound controls/logs](measured-3f2d53594f/controls.json). No source or native
mutation survivor is hidden; no native mutation was run in this audit.

Withdrawn or bounded claims are retained explicitly: OKX's immediate-eager POST,
WEEX refresh timing, FORCED output claims from enum/numeric-only surrogate
fixtures, complete class/SDK assembly, complete consumer enumeration, and paired
native runtime refusal/alias/scheduling proof. WEEX response parsing is not
runtime TS validation. The existing OKX lookup finding is not newly discovered.
The source loader's first exploratory attempt failed module resolution before
any profile ran; it was replaced by the existing maintained WEEX loader. That
uncommitted setup failure is not a SHA-bound gate or a parity finding.

No Cargo command, native test, workspace build/clippy, wasm lane, global parity
reader, full crate/workspace/monorepo suite, live exchange call or CI rerun has
been run for this audit. Prior PR #7914 measurements remain attached to their
own inputs and do not cover this new audit branch. Only the separately recorded
audit integrity/source/extractor commands may be claimed here.

The requested local incremental cache path was already absent with no local
compile running; cleanup reclaimed zero measured bytes. No target files are
tracked and no other worktree's cache was touched.

Audit metadata gates at `7dcfd8e6ab143aebc1ca07979dbc4d47176a14dc`: independent
frozen-git census/selection check **0**, deliberately corrupted selection hash
**1** at AUDIT-SELECTION, restored check **0**; source syntax **0**; extractor
check **0**. Output text reports 7,341 symbols/512 files, zero updates, zero
added/orphaned. [Static receipts](static-7dcfd8e6ab/gates.json) retain the actual
SHA and normalized published log hashes. These metadata gates do not establish
source/native semantic parity. No manifest was edited, so no extractor writer
was needed. Later citation/review/receipt-only changes do not relabel these
measurements as running at a later commit.

Local adversarial reviews are recorded in [reviews/](reviews/). The Kraken/WEEX
review returned MERGE at 7dcfd8e6ab. The OKX/control review initially blocked on
stale source citations; its final delta review returned MERGE at 476ed12d4c
after those citations were corrected. Both verdicts apply only to this finite
note audit. Reviewers inspected recorded evidence/code and reran no gates.
