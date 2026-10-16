# Polymarket: all eighteen divergent explanations

Frozen selection: `fada0ab29c9534bf4b51ec6e9c9385cf490b5b4c`. Integration parent actually merged before this slice: `333fd419fdbf70f081039610f22da9ee653bc96f` (merge commit `31627c69cf`). Selection is deterministic source-path/symbol order, all 18 divergent rows; stop at iteration18, `PolymarketPrice`. This is a floor over these explanations, not proof of every input or caller.

Venue census is **107 ported / 18 divergent / 16 pending** (141 rows); no status, target, identity, or file-target changes. The audit annotates 17 notes, preserving each previous note as historical text and independently freezing it in `selection.json`. No production code or existing native test changes are included.

Result: **1 supported / 17 findings / 0 unresolved**. Primary disjoint finding classes: **5 scheduling, 12 explanation**. The five scheduling rows also have secondary documentation issues; four have an additional retained-output string cost. Counts are row classifications, not 17 distinct code defects. Credentials and DeriveKeyResponse share one source producer. No whole-row promotion follows from a supported explanation.

Canonical declarations remain in [section14](../../MANIFEST-SPEC.md#shared-utf8-string-boundary), [section15](../../MANIFEST-SPEC.md#closed-typed-output-boundary), [section16](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [section17](../../MANIFEST-SPEC.md#numeric-ingress-defect), [section18](../../MANIFEST-SPEC.md#divergence-taxonomy), and [section19](../../MANIFEST-SPEC.md#forced-surrogate-conflict). Below are applications to observed source values, not duplicate policy declarations.

## Evidence and limits

`source-observations.mjs` AST-selects actual original method/function/constants bodies and executes them with recording callbacks. Native destinations and Rust String/decoder branches were **inspected only**. No new native refusal, native scheduling run, HTTP acceptance, full class composition, authentication flow, wallet, exchange connection, or payoff/risk calculation is claimed. The L2 header helper executes actual HMAC code with public all-zero test bytes; no credential or HMAC output is published. The normalization probe executes the actual public `placeOrder` rejection path with preflight capabilities and a shared Error realm; it never reaches signing/network.

At input `a6f6259514`, all18 source profiles pass; each targeted in-memory source fault fails a named runtime assertion at exit1, then the unmodified source passes again: **18 × 0/1/0**. Controls stop at profile18, `PolymarketL2Headers` (insertion order differs from manifest order). See [controls](measured-a6f6259514/controls.json) and its hashed logs. There are no setup failures or survivors in this measured control floor. A Gamma token-unit fault controls its row; the independently asserted DC00 outcome is not separately mutated. Duplicate credential profiles are not independent producer-body coverage. All unit/value assertions precede JSON output; there is no decode/re-encode equality gate.

Precision's support is narrow: it prices refusal versus numeric fallback2 for the two observed units. We withdraw any claim that every numeric input or every other string consumer has been proved. Similarly, a returned record with a unit is an observation at an injected method boundary, not proof that a live exchange emits it. Current schema declarations and actual method behavior are the reference surfaces.

Scheduling probes establish the source invocation prefix; Rust `pub async fn` body/request laziness is visible in source. The old notes price receiver injection but do not explicitly account for invocation-before-await versus poll timing or drop-before-poll. Constructor is now a ported body; class/factory assembly is separately pending. For the four record reads, `fetchOrderBook` and `createSignedOrder` are already ported, so the old blanket pending normalization/signing sentences are also stale. We do not equate any landed body with completed SDK composition.

## Per-row rulings

<a id="row-1"></a>

### 1. `PolymarketAPI.getMarkets` — finding (scheduling)

Actual method invokes the recording xhr callback once before its returned Promise is awaited; inserting one await before xhr changes that immediate count from one to zero.

The named Rust async function does not execute its body or start the HttpClient request until polled; creating then dropping the future loses the source invocation-prefix effect. Native execution was inspected, not run.

FINDING: receiver injection, owned records and typed number choices do not explicitly price this scheduling/lifetime loss. Historical constructor-pending language is stale: constructor is now ported; class/factory completion remains separate. Actual returned data[0].condition_id retains materialized unit D800. Required native String cannot retain that unit; refusal loses the entire market array, not just that field (inspected decoder). The prior CHOSEN-only framing does not account for this observed output under section19. Existing typed-shape/alias choices remain priced. Blanket pending normalization/signing language also needs narrowing: fetchOrderBook and createSignedOrder are ported, whereas fetchMarkets remains pending.

Source: `packages/safe-cex/src/exchanges/polymarket/polymarket.api.ts:252`. Native: `sdk-rs/safe-cex-venue-polymarket/src/public_reads.rs:20`. Profile: `getMarkets`. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict), [anchor](../../MANIFEST-SPEC.md#closed-typed-output-boundary), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="row-2"></a>

### 2. `PolymarketAPI.getOrderBook` — finding (scheduling)

Actual method invokes the recording xhr callback once before its returned Promise is awaited; inserting one await before xhr changes that immediate count from one to zero.

The named Rust async function does not execute its body or start the HttpClient request until polled; creating then dropping the future loses the source invocation-prefix effect. Native execution was inspected, not run.

FINDING: receiver injection, owned records and typed number choices do not explicitly price this scheduling/lifetime loss. Historical constructor-pending language is stale: constructor is now ported; class/factory completion remains separate. Actual returned data.asset_id retains materialized unit D800. Required native String cannot retain that unit; refusal loses the entire book including its levels, not just that field (inspected decoder). The prior CHOSEN-only framing does not account for this observed output under section19. Existing typed-shape/alias choices remain priced. Blanket pending normalization/signing language also needs narrowing: fetchOrderBook and createSignedOrder are ported, whereas fetchMarkets remains pending.

Source: `packages/safe-cex/src/exchanges/polymarket/polymarket.api.ts:274`. Native: `sdk-rs/safe-cex-venue-polymarket/src/public_reads.rs:33`. Profile: `getOrderBook`. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict), [anchor](../../MANIFEST-SPEC.md#closed-typed-output-boundary), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="row-3"></a>

### 3. `PolymarketAPI.getOrderBooks` — finding (scheduling)

Actual method invokes the recording xhr callback once before its returned Promise is awaited; inserting one await before xhr changes that immediate count from one to zero.

The named Rust async function does not execute its body or start the HttpClient request until polled; creating then dropping the future loses the source invocation-prefix effect. Native execution was inspected, not run.

FINDING: receiver injection, owned records and typed number choices do not explicitly price this scheduling/lifetime loss. Historical constructor-pending language is stale: constructor is now ported; class/factory completion remains separate. Actual returned data[0].asset_id retains materialized unit D800. Required native String cannot retain that unit; refusal loses the entire books array, not just that field (inspected decoder). The prior CHOSEN-only framing does not account for this observed output under section19. Existing typed-shape/alias choices remain priced. Blanket pending normalization/signing language also needs narrowing: fetchOrderBook and createSignedOrder are ported, whereas fetchMarkets remains pending.

Source: `packages/safe-cex/src/exchanges/polymarket/polymarket.api.ts:284`. Native: `sdk-rs/safe-cex-venue-polymarket/src/public_reads.rs:50`. Profile: `getOrderBooks`. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict), [anchor](../../MANIFEST-SPEC.md#closed-typed-output-boundary), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="row-4"></a>

### 4. `PolymarketAPI.getPrices` — finding (scheduling)

Actual method invokes the recording xhr callback once before its returned Promise is awaited; inserting one await before xhr changes that immediate count from one to zero.

The named Rust async function does not execute its body or start the HttpClient request until polled; creating then dropping the future loses the source invocation-prefix effect. Native execution was inspected, not run.

FINDING: receiver injection, owned records and typed number choices do not explicitly price this scheduling/lifetime loss. Historical constructor-pending language is stale: constructor is now ported; class/factory completion remains separate. Actual returned data[0].token_id retains materialized unit D800. Required native String cannot retain that unit; refusal loses the entire prices array, not just that field (inspected decoder). The prior CHOSEN-only framing does not account for this observed output under section19. Existing typed-shape/alias choices remain priced. Blanket pending normalization/signing language also needs narrowing: fetchOrderBook and createSignedOrder are ported, whereas fetchMarkets remains pending.

Source: `packages/safe-cex/src/exchanges/polymarket/polymarket.api.ts:326`. Native: `sdk-rs/safe-cex-venue-polymarket/src/public_reads.rs:72`. Profile: `getPrices`. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy), [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict), [anchor](../../MANIFEST-SPEC.md#closed-typed-output-boundary), [anchor](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

<a id="row-5"></a>

### 5. `PolymarketAPI.getServerTime` — finding (scheduling)

Actual method invokes the recording xhr callback once before its returned Promise is awaited; inserting one await before xhr changes that immediate count from one to zero.

The named Rust async function does not execute its body or start the HttpClient request until polled; creating then dropping the future loses the source invocation-prefix effect. Native execution was inspected, not run.

FINDING: receiver injection, owned records and typed number choices do not explicitly price this scheduling/lifetime loss. Historical constructor-pending language is stale: constructor is now ported; class/factory completion remains separate. The separate chosen f64 response restriction is explicitly priced; source actually returns an arbitrary object unchanged. This audit does not invent a forced retained-string output for the number API.

Source: `packages/safe-cex/src/exchanges/polymarket/polymarket.api.ts:258`. Native: `sdk-rs/safe-cex-venue-polymarket/src/public_reads.rs:114`. Profile: `getServerTime`. Policy: [anchor](../../MANIFEST-SPEC.md#divergence-taxonomy).

<a id="row-6"></a>

### 6. `normalizeTickSize` — finding (explanation)

With tickSize equal to materialized D800, actual normalizeTickSize throws; actual public placeOrder catches that Error and rejects with the same message units and emits it once. The unit at index33 (after Unsupported Polymarket tick size: ) is D800.

PrecisionInput::String and the Result error String cannot retain D800; current typed ingress refuses the tick before any comparable native error message is produced. This is inspected, not newly executed.

FINDING: CHOSEN-only wording does not account for the actual caller-inspectable rejection. Section19 applies to this returned Error.message, regardless of its simultaneous logging. No human-only scope was assumed.

Source: `packages/safe-cex/src/exchanges/polymarket/polymarket.exchange.ts:97; public placeOrder:603,650`. Native: `sdk-rs/safe-cex-venue-polymarket/src/utilities.rs:6`. Profile: `normalizeTickSize`. Policy: [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict).

<a id="row-7"></a>

### 7. `parseGammaMarketTokens` — finding (explanation)

Actual JSON.parse materializes three returned pairs: ordinary/Yes, D800/No, third/DC00. Token and outcome units are inspected before serialization; the ordinary pair remains present.

The Vec<(String,String)> output and ArrayString deserializer cannot retain the two observed units. Either malformed-for-String element aborts the array parse, so the helper returns empty: all three pairs, including the ordinary pair, are lost. Native branch inspected only.

FINDING: historical CHOSEN-only note already mentions whole-input refusal but omits the section19 ruling for returned token/outcome units and its larger three-pair loss.

Source: `packages/safe-cex/src/exchanges/polymarket/polymarket.exchange.ts:69; caller:385`. Native: `sdk-rs/safe-cex-venue-polymarket/src/utilities.rs:91`. Profile: `parseGammaMarketTokens`. Policy: [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict).

<a id="row-8"></a>

### 8. `precisionFromTickSize` — supported (input-boundary)

Actual function returns numeric fallback2 for materialized D800 and DC00 inputs. The same-class numeric fallback mutation2->5 fails its exact result assertion.

PrecisionInput::String refuses those units at ingress instead of producing fallback2. The original note explicitly prices that difference.

SUPPORTED explanation for this inspected input-boundary class only: the output is a number and retains neither unit. No forced retained-output claim or complete numeric-domain proof is made.

Source: `packages/safe-cex/src/exchanges/polymarket/polymarket.exchange.ts:90`. Native: `sdk-rs/safe-cex-venue-polymarket/src/utilities.rs:111`. Profile: `precisionFromTickSize`. Policy: [anchor](../../MANIFEST-SPEC.md#shared-utf8-string-boundary), [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict).

<a id="row-9"></a>

### 9. `PolymarketBalanceAllowance` — finding (explanation)

Actual getBalanceAllowance exposes materialized D800 in balance; direct charCodeAt/unit assertion occurs before serialization. The declared source field is string.

Required native PolymarketBalanceAllowance.balance uses Rust String and cannot retain D800. Current deserialization refuses the whole balance/allowance record; native decoder inspected, not rerun.

FINDING: historical CHOSEN-only string note states refusal but does not price this program-observed value and required String surface under section19. This is an explanation finding, not a newly measured native body fault.

Source: `packages/safe-cex/src/exchanges/polymarket/polymarket.api.ts:510`. Native: `sdk-rs/safe-cex-venue-polymarket/src/types.rs:157`. Profile: `PolymarketBalanceAllowance`. Policy: [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict).

<a id="row-10"></a>

### 10. `PolymarketClobMarket` — finding (explanation)

Actual getMarkets exposes materialized D800 in [0].condition_id; direct charCodeAt/unit assertion occurs before serialization. The declared source field is string.

Required native PolymarketClobMarket.condition_id uses Rust String and cannot retain D800. Current deserialization refuses the whole market record; the enclosing API decoder additionally refuses its entire array; native decoder inspected, not rerun.

FINDING: historical CHOSEN-only string note states refusal but does not price this program-observed value and required String surface under section19. This is an explanation finding, not a newly measured native body fault.

Source: `packages/safe-cex/src/exchanges/polymarket/polymarket.api.ts:252`. Native: `sdk-rs/safe-cex-venue-polymarket/src/types.rs:345`. Profile: `PolymarketClobMarket`. Policy: [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict).

<a id="row-11"></a>

### 11. `PolymarketCredentials` — finding (explanation)

Actual deriveApiKey exposes materialized D800 in apiKey; direct charCodeAt/unit assertion occurs before serialization. The declared source field is string.

Required native PolymarketCredentials.api_key uses Rust String and cannot retain D800. Current deserialization refuses the whole credentials record; native decoder inspected, not rerun.

FINDING: historical CHOSEN-only string note states refusal but does not price this program-observed value and required String surface under section19. This is an explanation finding, not a newly measured native body fault.

Source: `packages/safe-cex/src/exchanges/polymarket/polymarket.api.ts:522`. Native: `sdk-rs/safe-cex-venue-polymarket/src/types.rs:12`. Profile: `PolymarketCredentials`. Policy: [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict).

<a id="row-12"></a>

### 12. `PolymarketDeriveKeyRequest` — finding (explanation)

Actual deriveApiKey exposes materialized D800 in recorded config.headers.POLY_ADDRESS; direct charCodeAt/unit assertion occurs before serialization. The declared source field is string.

Required native PolymarketDeriveKeyRequest.address uses Rust String and cannot retain D800. Current deserialization refuses the whole request record at its typed ingress; no HTTP rejection was executed; native decoder inspected, not rerun.

FINDING: historical CHOSEN-only string note states refusal but does not price this program-observed value and required String surface under section19. This is an explanation finding, not a newly measured native body fault.

Source: `packages/safe-cex/src/exchanges/polymarket/polymarket.api.ts:522`. Native: `sdk-rs/safe-cex-venue-polymarket/src/types.rs:44`. Profile: `PolymarketDeriveKeyRequest`. Policy: [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict).

<a id="row-13"></a>

### 13. `PolymarketDeriveKeyResponse` — finding (explanation)

Actual deriveApiKey exposes materialized D800 in apiKey; direct charCodeAt/unit assertion occurs before serialization. The declared source field is string.

Required native PolymarketDeriveKeyResponse.api_key uses Rust String and cannot retain D800. Current deserialization refuses the whole response record; the same producer also witnesses Credentials; native decoder inspected, not rerun.

FINDING: historical CHOSEN-only string note states refusal but does not price this program-observed value and required String surface under section19. This is an explanation finding, not a newly measured native body fault.

Source: `packages/safe-cex/src/exchanges/polymarket/polymarket.api.ts:522`. Native: `sdk-rs/safe-cex-venue-polymarket/src/types.rs:77`. Profile: `PolymarketDeriveKeyResponse`. Policy: [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict).

<a id="row-14"></a>

### 14. `PolymarketL2Headers` — finding (explanation)

Actual buildL2Headers exposes materialized D800 in POLY_ADDRESS; direct charCodeAt/unit assertion occurs before serialization. The declared source field is string.

Required native PolymarketL2Headers.poly_address uses Rust String and cannot retain D800. Current deserialization refuses the whole header record; no network acceptance was tested; native decoder inspected, not rerun.

FINDING: historical CHOSEN-only string note states refusal but does not price this program-observed value and required String surface under section19. This is an explanation finding, not a newly measured native body fault.

Source: `packages/safe-cex/src/exchanges/polymarket/polymarket.signing.ts:118`. Native: `sdk-rs/safe-cex-venue-polymarket/src/types.rs:109`. Profile: `PolymarketL2Headers`. Policy: [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict).

<a id="row-15"></a>

### 15. `PolymarketOrderBook` — finding (explanation)

Actual getOrderBook exposes materialized D800 in asset_id; direct charCodeAt/unit assertion occurs before serialization. The declared source field is string.

Required native PolymarketOrderBook.asset_id uses Rust String and cannot retain D800. Current deserialization refuses the whole book, including otherwise retained levels; native decoder inspected, not rerun.

FINDING: historical CHOSEN-only string note states refusal but does not price this program-observed value and required String surface under section19. This is an explanation finding, not a newly measured native body fault.

Source: `packages/safe-cex/src/exchanges/polymarket/polymarket.api.ts:274`. Native: `sdk-rs/safe-cex-venue-polymarket/src/types.rs:246`. Profile: `PolymarketOrderBook`. Policy: [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict).

<a id="row-16"></a>

### 16. `PolymarketOrderBookLevel` — finding (explanation)

Actual getOrderBook exposes materialized D800 in bids[0].price; direct charCodeAt/unit assertion occurs before serialization. The declared source field is string.

Required native PolymarketOrderBookLevel.price uses Rust String and cannot retain D800. Current deserialization refuses the whole level; the enclosing typed book also becomes unavailable; native decoder inspected, not rerun.

FINDING: historical CHOSEN-only string note states refusal but does not price this program-observed value and required String surface under section19. This is an explanation finding, not a newly measured native body fault.

Source: `packages/safe-cex/src/exchanges/polymarket/polymarket.api.ts:274`. Native: `sdk-rs/safe-cex-venue-polymarket/src/types.rs:184`. Profile: `PolymarketOrderBookLevel`. Policy: [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict).

<a id="row-17"></a>

### 17. `PolymarketOrderResponse` — finding (explanation)

Actual postOrders exposes materialized D800 in [0].orderID; direct charCodeAt/unit assertion occurs before serialization. The declared source field is string.

Required native PolymarketOrderResponse.order_id (Option<String>) uses Rust String and cannot retain D800. Current deserialization refuses the whole response record; the observable field is orderID, not diagnostic errorMsg; native decoder inspected, not rerun.

FINDING: historical CHOSEN-only string note states refusal but does not price this program-observed value and required String surface under section19. This is an explanation finding, not a newly measured native body fault.

Source: `packages/safe-cex/src/exchanges/polymarket/polymarket.api.ts:398`. Native: `sdk-rs/safe-cex-venue-polymarket/src/types.rs:288`. Profile: `PolymarketOrderResponse`. Policy: [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict).

<a id="row-18"></a>

### 18. `PolymarketPrice` — finding (explanation)

Actual getPrices exposes materialized D800 in [0].token_id; direct charCodeAt/unit assertion occurs before serialization. The declared source field is string.

Required native PolymarketPrice.token_id uses Rust String and cannot retain D800. Current deserialization refuses the whole price record; the enclosing typed API decoder additionally refuses its entire array; native decoder inspected, not rerun.

FINDING: historical CHOSEN-only string note states refusal but does not price this program-observed value and required String surface under section19. This is an explanation finding, not a newly measured native body fault.

Source: `packages/safe-cex/src/exchanges/polymarket/polymarket.api.ts:326`. Native: `sdk-rs/safe-cex-venue-polymarket/src/types.rs:216`. Profile: `PolymarketPrice`. Policy: [anchor](../../MANIFEST-SPEC.md#forced-surrogate-conflict).

## Gates

At static input `8b5fa062448708b8755e88ccfd4ebe590e7d3f26`: extractor writer0 (zero updates/additions/orphans), integrity0, deliberate status-control1 at `POLY-AUDIT-ZERO-STATUS`, restored integrity0, Node syntax0, extractor check0 (zero would-update/additions/orphans). See `gates.json` for commands and log hashes. The initial local wrapper exited1 because it expected writer wording in the check output; its rejected receipt is retained. Reading the actual unchanged check output confirmed the check passed; no gate was rerun or relabelled. Source controls retain their separate input SHA above; later notes/receipt commits do not relabel them. The integrity gate compares this fixed selection and venue census against the frozen source and integration manifests, verifies all17 annotation suffixes, zero status/target moves, no production/native-test changes, and published control log hashes. Its deliberate in-memory status fault must fail its named assertion before restoration.

For this audit, **not run**: Cargo check/test/build, workspace build, host clippy, wasm build/clippy, native mutation controls, full TypeScript checking, full monorepo suites, global parity readers, CI, live/authenticated exchange traffic. Prior backlog build/native receipts belong to their own input SHAs and do not cover this audit. The requested local incremental cache was already absent; no native compile has recreated it.

Sub-agent reviews: both row scopes returned MERGE at `8b5fa06244`; the receipt-only delta returned MERGE at `808282db57` after checking all six static log hashes. Reviewers executed no gates. See `reviews.json`. Later README trailing-space cleanup and this review receipt do not expand their reviewed SHAs or the measured gate inputs.

Pre-push integration refresh: merged `d4e20845a85cc4e2711c99648ef7547f8aa806df` at `aa16ffab3dc01c92b4fecece7f7ead325355f7e5`; upstream changed none of the Polymarket source/native/manifests or policy anchors audited here. At that exact merge SHA, integrity0 / status fault1 / restored0, extractor check0 (zero would-update/additions/orphans), and diff check0 passed. See `merge-gates.json`. Earlier source and static gate SHAs remain unchanged; native/workspace/host/wasm/CI gates and source controls were not rerun for this merge. Only integration-merged work is inherited; no unrelated unreviewed topic ancestor is introduced.
