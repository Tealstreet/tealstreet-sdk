# Coinbase notification reachability review

## Production binding follow-up

The production binding supplies a native Store publication bridge and witnesses
startup commit-before-failure, subscriber cause identity and stopped fanout. The
fourteen existing notification finding roots remain individually classified; this
new startup witness does not promote the other roots or reprice their audit floor.
See [binding gates](BINDING_GATES.md) for the executed scope.

The ES256 primitive in #148 reclassified `getSigningKey` and `sign` as CHOSEN
platform-error/nonce output choices. Both remain in the notification walk under
an exact two-root `crypto-choice` classification. The current 262-root census is
246 ported, fourteen notification findings and two crypto choices. A negative
control refuses assigning that choice to any other root. All thirteen direct
Store sites and the 29 finding-root call paths remain pinned.

## Concrete snapshot error bridge

The resumed tomllib census at merged parent `a58f9ce8f1650bfb3d88347cb3e9071ab39b44ae`
was 392 ported / 34 divergent / 0 pending across 16 Coinbase manifests (426 rows).
The brief's 426-pending figure was stale. The merged origin parent was
`1810a76a9764020538782a82b29780b98c9778d1`. That older origin itself had
393 ported / 33 divergent / 0 pending: the local snapshot withdrawal had not yet
reached that integration tree. No gain is claimed against that older census.

Before delivery, batch `53ee502232b005173d8a6f3f0d7cf94bad7f072c` was merged at
`b575df520a974c61587111341dc290fbb7722f67`. Batch 53ee includes the snapshot
withdrawal and measures 392 ported / 34 divergent / 0 pending. Against that exact
parent this repair moves one divergent/FINDING row to ported, zero pending rows,
with no added or removed rows. The TOML helper-exemption conflict preserved both
parents' appended groups (1,802 distinct entries); Coinbase code, tests, scripts
and Store code are byte-unchanged from source proof `b654fe814138b5069b22e6246f879f9aad537ac5`.

`PortfolioThrown` now retains an original `StoreMutationError` separately from
JSON throws. Its native accessor and standard error chain expose subscriber
identity, metadata and the original cause without reconstructing a JSON error.
Existing JSON callers use `from_value` with their original unchanged payloads.
An imported native Store publisher uses
`Store::update(...).map_err(|error| Rc::new(PortfolioThrown::from(error)))`;
converting the error to `Value` would discard the origin this repair preserves.
This does not bind native property access in unrelated catch/message-policy bodies.

The concrete `CoinbasePrivateWebsocket::apply_order_snapshot_batch` already
propagates the imported publication failure unchanged. The new native test drives
that entry through class `on_message` with an actual `DefaultStore` publication.
It directly asserts committed order/loaded values inside the failing subscriber,
assembly reset before notification, original subscriber and cause identity,
retained failure/order roots, suppression of later subscribers, host work and
following user events, and recovery after unsubscribe. The test keeps the class
Frame/order roots alongside its typed Store witness under a fixed imported Store
binding. Live replacement of that imported Store during a loaded getter is not
established by this fixture. It does not claim that the
typed Store projection is a full alias-preserving exchange runtime binder.

The live source class oracle adds a sixth named scenario, `notification`, ending
at recovery after the first throwing subscriber is removed. Native assertions
precede its source projection. This iteration closes only
`CoinbasePrivateWebsocket#applyOrderSnapshotBatch`: 393 ported / 33 divergent /
0 pending. The same 262 reviewed roots remain; 248 are ported and fourteen retain
notification findings, with 29 call paths rooted in those finding rows. Some
snapshot subpaths are now supported; this root-path count is not a defect count.
All thirteen direct Store sites
remain content-pinned. The audit checks the new native-origin representation,
while compiled mutations and scoped tests provide behavioral proof; see
[GATE_EVIDENCE.md](GATE_EVIDENCE.md) for measured revisions and exits.

Other notification findings remain open because their concrete bindings and
error-property/message policies have not been established. The order-event row
also retains its separately recorded raw-ID/coercion finding. Shared carrier
capacity alone does not close those rows. Classification follows the canonical
[taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy); shared boundaries remain
at [strings](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[outputs](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[aliases](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[numeric ingress](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), and
[surrogate observability](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

## Health capability carrier audit after #691

The degraded/auth-failed health methods return the existing `PortfolioFailure`
carrier because both Store notifications and log/warning callbacks can fail.
Message-received and standalone snapshot publication retain their direct
`StoreMutationError` results. The source audit checks each exact outer signature,
fallible callback signatures, unchanged native Store origin bridge, and `?`
propagation from log/warning calls. It does not accept a callback's Result as
evidence of the enclosing method's return type. Five negative controls reject
flattened health errors, infallible callbacks and discarded callback failures;
all existing notification classifications and controls remain intact.

## Original audit, before the snapshot bridge repair

The review starts from integration `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`. Its 262 ported callable rows are retained in `scripts/store-notification-reviewed-roots.json`, including every subsequently reclassified row. The current audit walks all 262 roots; it does not shrink the cohort when a finding is recorded. Current ported callables number 247 after the class snapshot publication withdrawal below.

Four roots are actual typed function properties assigned by constructors: `CoinbaseStarbaseWebsocketSessionController#replaySubscriptions`, `CoinbaseStarbaseAuthSession#createCdpJwt`, `CoinbaseStarbaseAuthSession#clock`, and `CoinbaseStarbaseJsonRpcClient#transport`. The collector resolves direct function types, optional/parenthesized function types and local callable aliases. It also handles TypeScript constructor parameter properties. These declarations have no local implementation body; injected implementations and arbitrary dynamic dispatch outside the venue are outside this static source walk. No synthetic source declaration or method-name exemption is used.

The expanded walk exposed 29 paths from fourteen previously ported roots. Seven direct writer rows account for ten source calls. Three other direct writer rows already expose the concrete fallible Store contract, bringing the source inventory to thirteen calls in ten rows. The separately indexed auth-failure health method delegates to a typed writer.

| Direct writer | Source operation / sites | Native review |
| --- | --- | --- |
| CoinbaseExchange.start | Store.update, markets | LifecycleHost.update returns LifecycleAsync through PortfolioFailure, without a concrete Store notification origin. Both source and native start await the update; the finding concerns the native notification carrier and concrete binding. FINDING. |
| CoinbaseExchange._startPublicDataOnly | Store.update, public state | LifecycleHost.update returns a Promise that this body discards. Notification failures must be synchronous outer errors. Concrete Store commit/failure behavior is unproved. FINDING. |
| CoinbaseExchange.tick | Store.update, tickers/balance/positions/orders | TickHost.update returns PortfolioFailure, which holds JSON only. Whole-error forwarding cannot retain the native subscriber cause. Existing update fixtures always succeed at these four sites. FINDING. |
| CoinbaseExchange#reconcileOpenOrdersFromRest | Store.update, reconciled orders | Discards the update Promise; forwards the whole failure through a JSON-only carrier. No lossless native notification bridge. FINDING. |
| CoinbaseExchange.cancelOrders | Store.updateOrdersStatus, each cancel batch | Source logs and consumes the failure. Existing injected-error tests prove that message policy, but do not prove real committed Store state, later-subscriber suppression, or stopping subsequent cancellation/timer effects after a native notification error. FINDING. |
| CoinbaseExchange.placeOrder | Store.addOrUpdateOrder, optimistic order | Source logs then constructs a new Error. Original-cause retention in the returned error is therefore not required. Real Store commit/subscriber behavior and suppression of subsequent timers remain unproved. FINDING. |
| CoinbasePrivateWebsocket.handleUserEventOrders | Store.addOrUpdateOrders, live update | PrivateOrderEventHost uses PortfolioFailure, and its existing Store fixture always succeeds. Native-origin retention and suppressing later fill events after a notification failure remain unsupported. FINDING. |
| CoinbaseExchange.markPrivateUserStreamMessageReceived | Store.updateSocketHealth | Concrete native StoreMutationError return, committed-state/subscriber ordering witnesses already exist. Retained as ported. |
| CoinbaseExchange.markPrivateUserStreamDegraded | Store.updateSocketHealth | Concrete native StoreMutationError return, error occurs before later log/warning effects. Retained as ported. |
| CoinbasePrivateWebsocket#applyOrderSnapshotBatch | Store.update, final snapshot | The standalone helper returns native StoreMutationError with reset/committed-state witnesses. The added concrete class entry publishes through CoinbasePrivateClassHost::publish_private_order_snapshot returning JSON-only PortfolioFailure; that path cannot retain native subscriber identity/cause. The source row is withdrawn to divergent/FINDING; the standalone helper is preserved. |

The additional seven finding roots are `_startPrivate`, `#schedulePrivateUserStreamWriteCheck`, `#scheduleOpenOrdersRestReconciliation`, `updateOrder`, private `onMessage`, `#handlePrivateStreamError`, and `handleUserEvent`. Their precise paths and native bridge are named in their own manifest notes. Three health rows remain supported. The standalone typed snapshot helper also remains proven, but does not justify whitelisting its source row once the additional class implementation is mapped. Adding the fifteen unsupported roots to the supported-row whitelist would falsely claim that their JSON-only error boundary retains the native cause.

The native evidence is `PortfolioThrown.value: Option<Value>` and `CoinbaseOrderFailure.value: Option<Value>` versus `StoreMutationError::Notification`, whose StoreNotificationError retains the Store-local subscriber identity and whose StoreSubscriberError can retain an original `Rc<dyn Error>`. The typed cause cannot pass through these JSON-only carriers. Lack of a literal StoreMutationError return is not itself a defect: cancel/place deliberately consume or replace the error. Their findings concern missing concrete Store behavior witnesses, rather than an invented requirement to return the original cause.

All fifteen notification rows are explicitly divergent/FINDING with Coinbase as owner. Classification follows [the canonical taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy). Other shared declarations, when applicable, remain at their canonical [string boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [output boundary](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [alias boundary](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [numeric ingress](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), and [surrogate observability](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict) anchors.

The gate still rejects every unsupported ported notification path. A finding cannot be promoted to ported without updating its independently reviewed state. Every recognized direct Store call, including four distinct tick sites, is fingerprinted: adding or changing a recognized call inside an already acknowledged finding still requires review. The scanner recognizes direct properties, computed string members, and simple local aliases. Destructured aliases, .call/.apply and arbitrary dynamic dispatch remain outside its lexical analysis. New ported callables cannot bypass the complete reviewed cohort. Source declarations are counted independently of the root array.

The executable test surface checks actual 262-root results and named negative controls: dropped typed properties, dropped aliases, stale census, same-length root substitution, unaudited new writer, changed reviewed call site, premature finding promotion, no-op gate, and missing compiler. It also runs from a source mirror without node_modules in any ancestor and resolves the compiler through SAFE_CEX_TYPESCRIPT_PATH. These are Node audit gates; they do not establish Rust notification ABI parity or full dynamic JavaScript reachability.

Review was local and adversarial through a sub-agent. No human checkpoint applies. The recorded omissions are the fifteen notification contracts named above; this repair records them without waiting for an ABI repair or falsely claiming they are implemented.

The additional snapshot finding was identified during adversarial mapping review of the native class closures. It adds one direct root-to-writer path to the previous 29-path floor, yielding thirty reviewed finding paths in this iteration. The audit still walks all 262 source roots and all thirteen content-pinned direct calls. The prior typed snapshot-helper proof is not extended across the class JSON error carrier. A named negative control promotes this exact row and fails the reviewed-status assertion; restoration must pass. This is one status withdrawal during implementation review, separate from the later explanation-only divergence audit.
