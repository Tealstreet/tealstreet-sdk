# Coinbase divergence audit: all 33 rows

Audit parent: `7216d14d553d5415600a8d45176411b4f3981a83`, after merging
integration `0286f63437e4320e8c12886d5fef8fc6dfa2eac3` into the lane branch.
The earlier snapshot repair and its gates remain separate ancestry; no old gate
exit is attributed to this final audit commit.

Independent `tomllib` census: **393 ported / 33 divergent / 0 pending**, 426 rows
in 16 manifests. All 33 divergent identities were inspected, sorted manifest
filename then source order, index 0 `CoinbaseExchange` through index 32
`CoinbaseSpotExchange` in `coinbasespot.exchange.ts.toml`. This completes this
frozen annotation audit, not whole-venue behavioural parity.

Final disjoint totals: **11 supported explanations / 8 behaviour findings /
14 rationale or proof findings = 33**. All 22 findings remain open: **36.36%
behaviour / 63.64% rationale or proof**. The original 16 were 5/11; six additional
explanations were withdrawn (three behaviour, three rationale). Two supported
notes were corrected: the label validator now cites the canonical declaration,
and normalized-order serialization is attributed to DTO declaration order,
not sorting by serde_json::Map (the workspace enables preserve_order).

**Zero status rows moved, zero pending rows moved and zero ported rows gained.**
Only eight notes changed; all 426 statuses, source identities, layers, kinds and
Rust targets are preserved. The manifest writer reported zero updates/additions/
orphans after these annotations. This is a writer result, not an extractor-check
or CI-parity pass. The [exact ledger](evidence/divergence-audit/ledger.json)
records the independent expected row classes, note hashes and actual targets.

Behaviour identifies a concrete wrong local operation or missing class join.
Rationale/proof identifies an unsupported choice/observation or missing concrete
binding witness; those fourteen are not proven-correct bodies. Dependency defects
are recorded separately from imported-capability forwarding bodies, so this row
split is not a count of failing runtime call paths or independent root causes.
Supported explanations remain bounded to their recorded causes and witnesses.
The new alias losses are static source/native findings; no new runtime failure,
production mutation control or all-input proof is claimed for this audit.

| Index | Source row | Disposition | Evidence / remaining obligation |
| ---: | --- | --- | --- |
| 0 | `CoinbaseExchange` | behaviour | Missing concrete exchange/API/socket/startup/tick/Store/local-method class join; partial portfolio repair remains bounded. |
| 1 | `CoinbaseExchange.start` | rationale/proof | Await/outer-error handling exists; concrete Store startup publication and error binding remain unwitnessed. |
| 2 | `CoinbaseExchange._startPrivate` | rationale/proof | Local body awaits the fallible tick capability; concrete downstream binding remains unwitnessed. |
| 3 | `CoinbaseExchange._startPublicDataOnly` | rationale/proof | Synchronous outer Result propagates before the returned Promise is dropped; real Store commit/fanout witness absent. |
| 4 | `CoinbaseExchange.tick` | behaviour | Catch reads only e.value.message although retained native Store failures have value=None; message is lost. |
| 5 | `CoinbaseExchange#schedulePrivateUserStreamWriteCheck` | rationale/proof | Timer returns the health callback Result directly; typed Store health binding/witness absent. |
| 6 | `CoinbaseExchange#reconcileOpenOrdersFromRest` | rationale/proof | Synchronous update error is caught and original failure forwarded; concrete Store/callback witness absent. |
| 7 | `CoinbaseExchange#scheduleOpenOrdersRestReconciliation` | rationale/proof | Hot reconciliation is scheduled; downstream concrete notification binding remains unproved. |
| 8 | `CoinbaseExchange.cancelOrders` | rationale/proof | Source consumes the exception; real commit, stopped fanout/batches and timer suppression remain unwitnessed. |
| 9 | `CoinbaseExchange.updateOrder` | behaviour | Native edit catch reads the empty JSON view and may log literal undefined instead of the retained native message. |
| 10 | `CoinbaseExchange.placeOrder` | rationale/proof | Source replaces the exception; real optimistic Store commit/fanout and timer suppression remain unwitnessed. |
| 11 | `mapCoinbaseRestOrder` | behaviour | Source info aliases rawOrder; native orders.rs stores raw_order.clone(). Existing status/layout choices omit that alias loss. |
| 12 | `buildCoinbaseStarbaseBuy` | behaviour | Source Buy spread retains nested size/otoco aliases; Rust expanded intent deep-clones them. No priced choice or paired alias witness. |
| 13 | `buildCoinbaseStarbaseSell` | behaviour | Source Sell spread retains nested size/otoco aliases; Rust expanded intent deep-clones them. No priced choice or paired alias witness. |
| 14 | `buildCoinbaseStarbasePlaceOrder` | supported | Blocked typed shape/declaration-order choice is priced and witnessed; this direct builder borrows intent rather than deep-cloning a side wrapper. |
| 15 | `validateLabel` | supported | Source producer independently asserts 64-D800 success and 65-D800 failure; native tests assert ingress refusal and scalar/astral length cases. Canonical citation repaired. |
| 16 | `normalizeCoinbaseStarbaseOrder` | supported | Native tests price extra-field loss, info alias snapshot, nonfinite info and typed declaration order. False Map-sorts mechanism corrected; preserve_order is enabled. |
| 17 | `resolveChartResolution` | supported | Explicit deliberate rejection of source inherited function/object resolution values; own-key literal resolution remains the selected contract. |
| 18 | `buildCoinbaseStarbaseChartData` | supported | Inherited-resolution correction is priced alongside validation/request cases; no whole-function completeness inference. |
| 19 | `buildCoinbaseStarbaseChartChannel` | supported | Source inherited values enter channel strings; native own-key rejection is explicitly deliberate and witnessed. |
| 20 | `MARKET_RESPONSE` | supported | Numeric repair, returned declared-string unit refusal, ignored-field whole-input refusal and depth floor are separately witnessed/priced. |
| 21 | `INTX_INSTRUMENTS_RESPONSE` | supported | Same bounded numeric/string/ignored-field framework, with independent instrument field paths and actual native assertions. |
| 22 | `PORTFOLIO_BREAKDOWN_RESPONSE` | supported | Same bounded framework, with independent portfolio field paths and actual native assertions; no exact resource-cutoff claim. |
| 23 | `resolveCoinbaseUsdEquivalentMarket` | supported | Source inherited market values versus native own-key rejection are explicit; nullish/own-value precedence has a separate corpus. |
| 24 | `parseCoinbaseCredentialJson` | rationale/proof | Acceptance boolean does not establish exact materialized returned x unit; current returned-object attribution lacks the required observation. |
| 25 | `normalizeCoinbaseApiCredentials` | rationale/proof | Ignored-property choice is witnessed; a separate source-accepted organization/key identifier can return a lone unit in keyName, without paired native/source proof. |
| 26 | `base64UrlEncodeString` | rationale/proof | String corpora omit materialized lone-unit TextEncoder replacement bytes/ASCII base64url output; input explanation lacks its local output witness. |
| 27 | `pemToPkcs8Der` | supported | Unit error deliberately excludes platform DOMException metadata; byte/rejection tests witness the declared PEM/base64/SEC1 policy. |
| 28 | `CoinbasePrivateWebsocket.onMessage` | rationale/proof | Snapshot native failure is witnessed; remaining health/auth/live-order imported bindings are unproved. Transitive raw-order defect stays separate. |
| 29 | `CoinbasePrivateWebsocket#handlePrivateStreamError` | rationale/proof | Health Result is forwarded directly; concrete native auth-failure publication binding/witness absent. |
| 30 | `CoinbasePrivateWebsocket.handleUserEvent` | rationale/proof | Health/order Results propagate and stop later work; remaining concrete bindings are unproved. |
| 31 | `CoinbasePrivateWebsocket.handleUserEventOrders` | behaviour | Absent IDs become literal undefined, collapsing source-distinct keys; parent ID, raw product coercion and UTF16 narrowing remain open. |
| 32 | `CoinbaseSpotExchange` | behaviour | Spot override routing is repaired, but wrapping the partial base class does not supply its missing concrete runtime join. |

Existing inspected witnesses include tests/orders.rs, write_order_builders.rs,
write_guards.rs, normalize_order.rs, public.rs, pure.rs, numeric/string decoder
targets, credentials.rs, crypto_encoding.rs and signing_helpers.rs. Their actual
source producers were inspected. In particular write-guards.cjs asserts both
64/65-D800 outcomes; credentials.cjs's parser surrogate flag is only a boolean,
and its normalization surrogate witness puts the unit in an ignored field.
Those are different observation scopes. The three new alias rows lack their
own paired mutation-after-call evidence; source shallow retention versus native
clone is established by code inspection. The older numeric and snapshot runtime
receipts retain their original SHAs; no pass is transferred to this audit SHA.

Sub-agent review independently proposed all six withdrawals and the two note
repairs. All selected native target files and final declaration names exist;
this lexical/static check does not certify the missing class runtime joins.
The final two gates are recorded by exact SHA in
`.aimux/coinbase-finding-handoff/<SHA>/gates.json` at the worktree root. Host and
wasm clippy, wiring, layers and routing were not run here; the coordinator owns
them. No new Rust tests/source oracles, workspace test suite, extractor-check,
or CI run was executed for this annotation audit. CI parity completion remains
unconfirmed; an interrupted gate is never counted as a pass.

Canonical declarations remain in MANIFEST-SPEC
[14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), and
[19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).
