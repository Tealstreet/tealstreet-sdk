# Crypto.com divergence audit

The subsequent [DTO retention repair](DTO_RETENTION.md) supersedes the eleven open DTO findings below; this document retains its historical measurements.

Historical snapshot: [subsequent behaviour repairs and current triage](BEHAVIOUR_REPAIRS.md) supersede its open-finding counts; earlier measurements remain attached to their original revisions.

This is the original audit snapshot. The later [signing execution repair](SIGNING_EXECUTION.md) updates the timing mechanisms and current finding classification; the counts and SHA-bound observations below retain their historical scope.

Measured integration parent: `d4e20845a85cc4e2711c99648ef7547f8aa806df` (merged PR7872, including the PR7854 indexer fix). PR7872 already closed the five pending rows and the two original socket-class FINDING rows. The original20 divergences therefore leave **18 divergent rows** for this audit. The independently frozen [cohort](evidence/divergence-audit/cohort.json) records source identities, original annotation hashes and existing mappings. Cohort enumeration order is ascending SHA256 of `divergence-audit:` + source path + `::` + symbol; the enumeration floor reaches iteration17, `CryptocomAccountsResponse`. All18 were inspected. No status, layer, target or Rust production source changes are part of this audit. Crypto.com owns all17 open audit findings.

After scope correction: **1 supported explanation (5.6%), 17 findings (94.4%)**. The fleet finding TYPE split is **13 explanation /3 scheduling /1 behaviour /0 documentation /0 already-known**. The former16 justification findings are refined into13 explanation and3 scheduling; these are not three new findings or newly proved live scheduler bugs. None of the original18 notes was adequate unchanged. Supported residual causes inside a finding do not turn the entire row into a supported explanation. The annotations preserve those residuals while withdrawing the unproved claims. This is an explanation audit, not an exhaustive conformance certificate. The known canonical baseline also contains77 diagnostics naming Crypto.com; this audit does not repair those mapping/index defects.

The governing declarations remain in MANIFEST-SPEC: [§14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [§15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [§16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [§17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [§18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [§19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict). This report records applications and observations, not replacement declarations.

## Backlog closure and finding types

The independently checked history is **112 ported /20 divergent /5 pending at ac0f5fccf2 →119/18/0 after merged PR7872**. The five pending rows were `createAPI`, `CryptocomExchange`, its constructor, `start` and `tick`; the two original FINDING rows were `CryptocomPrivateWebSocket` and `CryptocomPublicWebSocket`. All seven are now ported. [The historical closure census](evidence/completion-62169a9fcb/census.json) pins their status changes at62169a9fcb; [the original gate sheet](GATE_EVIDENCE.md) retains the23 runtime controls and their separate measured SHA, rather than treating65 source-entry smoke checks as full class parity. The provisional exchange-class finding created during that earlier implementation is not counted twice. This audit/reporting branch moves zero status rows.

| Fleet finding type | Rows | Scope |
| --- | ---: | --- |
| explanation |13|private-write helper, interval mapper and eleven DTOs|
| scheduling |3|generateSignature, generateAuthenticatedRequest, generateWsAuthMessage|
| behaviour |1|isPublicEndpoint mutable table difference|
| documentation |0|no remaining clerical-only finding in this cohort|
| already-known |0|no duplicate previously open finding; the two original socket findings were closed before this cohort|

The supported row is `paramsToString`. These mutually exclusive primary types describe the audit findings, not behaviour-equivalence verdicts. An explanation finding means the rationale has not been established; it supplies no positive behaviour-parity evidence by itself. Scheduling rows retain unresolved invocation/poll timing policy. The concrete mutable-table behaviour difference is kept separate. [Per-row types and closure history](evidence/divergence-audit/fleet-finding-types.json) preserve the original18-row identity floor, ending iteration17 AccountsResponse. Historical SHA-bound receipts keep their original terminology. The three timing differences were documented already; this audit classifies their unresolved justification rather than claiming three newly discovered effects. Already-known0 counts duplicate previously open findings, not previously described mechanisms.

## Direct TypeScript comparison evidence

At **501678668536dcdcf83869592f2a2ffd9c7a7ea4**, the executed witnesses directly compare Rust observations with the maintained executable TypeScript for **7 distinct rows**: both API helpers, `paramsToString`, the three signature/auth builders and `mapCandlestickInterval`. This row count is independently enumerated in the linked receipt; it is not inferred from the coincidentally seven passing Rust test functions. There are also **11 erased interface rows** with compiler-schema, actual `JSON.parse` and decoded-native-value evidence, totaling269 positive DTO cases. Interfaces have no executable TS method body, so those eleven are reported separately.

The executable comparison floors are29 URLs per API helper (ending index28 `/public/not-real`),280 params/level pairs (index279 `number_array`, level2.5),14 cases per signature/auth builder (global auth indices37/39/41 respectively), and31 interval observations (index30 `toLocaleString`). Native results/fields/errors are compared within these corpora. Auth comparisons measure results and post-poll clock counts; they do not erase the eager-vs-deferred difference. Request and WS builders have explicit pre-poll clock observations; signature deferral follows actual async-function inspection and ready-poll output tests, rather than a new pre-poll side-effect measurement. Interval inherited-member projections do not prove function/object referent identity. API ordinary-table matches do not erase the separately measured mutable-table mismatch. Source-code-unit and excluded-domain probes remain distinct from ordinary passing comparisons.

Thus **7 rows have direct executable-reference comparison evidence; zero rows are claimed exhaustively behaviour-equivalent**. The11 interface rows supply additional bounded data-contract confidence. These counts apply to the audited18-row cohort, not a venue-wide census of every source-backed row. Neither13 explanation findings nor one supported explanation is a count of verified behavioural parity. Exact row/test/case scopes and measured SHA are in [the confidence receipt](evidence/divergence-audit/fleet-finding-types.json).

## API helpers

`isNonRetryablePrivateWrite` is an explanation finding. Its outgoing chain is local endpoint lookup → substring test → bool. No string is returned. Sixteen lone-unit placements (D800/DFFF, public/private endpoint, prefix/suffix/query/interior) produce64 source boolean observations including replacement comparisons; the floor stops at index15, DFFF inside `private/create-order`. These cases show no change when replacing the unit. They do not prove all inputs, but they do invalidate treating the mere `str` argument as demonstrated observable loss. The older29 URL/58 boolean corpus remains a separate floor. Incoming retry/dispatch callers do not add outgoing host seams.

`isPublicEndpoint` is a **behaviour finding**: source reads the exported mutable `PUBLIC_ENDPOINTS` array on each call. Appending `/audit/custom` changes false→true; removing BOOK changes true→false. Rust reads a fixed slice: the new witness first asserts native custom=false and BOOK=true, then executes the complete source API and types modules and asserts the source mutations. The finding is retained; this audit does not implement shared mutable endpoint configuration. By contrast the private-write helper captures its endpoint array at module initialization: replacing `ENDPOINTS.CREATE_ORDER` later does not update its captured member. The public-table finding must not be copied onto that helper.

## Signing chain

`paramsToString` has the one supported, corrected explanation: the scoped [signing data choice](../parity/MANIFEST-SPEC.md#cryptocom-signing-data-domain) remains, while its observable string cause is classified through §19. The actual source JSON parser materializes object keys D800 and E000. Params traversal emits units `[55296,49,57344,50]`; the native `Value` key surface cannot retain D800 and JSON ingress refuses the entire params object. Explicit FFFD replacement orders E000 before FFFD. The downstream public `generateSignature` produces different HMACs: source `54a13805…e41826`, replacement/native `5abd9e4c…306899f`. The new witness independently pins both complete digests and native params order, not re-encoded bytes. Isolated D800 versus FFFD in method or secret converges after HMAC encoding in the measured cases; no blanket signature difference is inferred.

Outgoing traversal is recursive params/UTF-16 sorting → shared JS coercion → preimage → `sign_payload(None)` → shared local `hmac_sha256` → hex result. No remote signer, Transport, Store or notification path is reachable here. Numeric capture in the witnesses and incoming adapters is not a production helper's own outgoing recursion budget. Existing params evidence covers280 cases, ending index279, number_array with level2.5, in its independently maintained inventory; this audit does not extend that floor into arbitrary executable JS values.

`generateSignature`, `generateAuthenticatedRequest`, `generateWsAuthMessage` are **scheduling findings (unpriced execution policy)**: the original explanation claimed that synchronous execution required either bypassing the public signer or adding another crypto implementation. Eager first-poll driving of the local public signer is a counterexample to that disjunction; shared synchronous `hmac_sha256` also exists. The unproved necessity claim is withdrawn centrally in [the execution declaration](../parity/MANIFEST-SPEC.md#cryptocom-signing-execution-boundary). Source calculation/clock reads occur before return; native futures defer them until poll. Local first-poll readiness does not justify the policy choice. Existing42 auth cases (ending index41, private/deep-error input, ws, explicit=true) remain bounded behavior evidence, not proof of that choice.

Request results retain source method and api_key units D800, while the required native fields are borrowed `str`. Native params retain pointer identity, not an owned clone. Source mutation after return remains visible through the old request; the native borrow excludes concurrent mutation. The new witness directly checks pointer identity and source post-return counter7 before serializing source observations. The host clock callback is the traversal boundary; the audit's constant clock import is an explicit probe host. Ws auth returns the same observed source key unit and always signs empty params; arbitrary params-domain exclusions do not explain that helper. HTTP serialization, retry, actual exchange engines and socket dispatch are outside this witness.

## Interval and wire records

All12 rows in this group are explanation findings. That classification does not establish behaviour parity. A representation cost does not establish why source parity would be worse. The old generic typed-boundary/unknown-fields explanation is withdrawn; supported observed losses are retained. No outgoing closed shared-enum consumer was demonstrated. The typed helper differs from `BoundHost`’s `ReadHost::interval`, whose generic imported interval callback is a distinct path.

For `mapCandlestickInterval`, source `listenOHLCV` can throw a caller-inspectable error retaining D800 in its message; the native error is `String`. That probe is outside the declared Timeframe union and is not presented as a valid typed argument. Inherited Object.prototype keys also return actual function/object identities in source and are forwarded to the subscription callback; native `TableHit` metadata is a distinct representation (named `ts-bug` compatibility). The existing31-case interval floor stops at `toLocaleString`. A human-only error-label assumption is not supported.

The11 DTOs have source JSON.parse observations of retained D800 and native whole-record refusal, including loss of good fields. Their successful witness path is shared raw-string numeric capture → private remote deserialization/strict field decoding → native fields/exhaustive numeric output → observation. A D800 input can be refused during that initial capture (`JsNumberInput` delegates to `RetainedJsonInput`, whose `Text` variant holds a `String`), before the private decoder runs; the audit does not assert that the private guard executed on that input. The production generic host methods retain `V`; no adoption of these DTO decoders by REST/WS methods was found. Therefore refusal is attributed to these DTO/decoder surfaces, not asserted as a failure of a production REST/WS call. Only the registered raw-string numeric entry is evidenced; bare serde, owned/borrowed deserializer alternatives and all numeric spellings are not certified.

| Row | Materialized source field | Positive case floor | Final positive iteration |
| --- | --- | ---: | --- |
| CryptocomInstrument | symbol |26|one_hot:margin_sell_enabled|
| CryptocomBook | instrument_name; data[1].asks[0][1] |25|data[1].t:1.0000000000000002|
| CryptocomTicker | i |8|t:1.0000000000000002|
| CryptocomTrade | i |38|dataTime:1.0000000000000002|
| CryptocomCandlestick | o |8|t:1.0000000000000002|
| CryptocomPositionBalance | instrument_name |2|one_hot:collateral_eligible|
| CryptocomBalanceSummary | position_balances[1].instrument_name (also outer instrument_name) |3|two balances|
| CryptocomOrder | present optional account_id |89|one_hot:post_only|
| CryptocomAccountsResponse | this_account.uuid |3|quote/backslash/NUL uuid|
| CryptocomTradeFill | account_id |59|literal:liquidity_indicator:TAKER|
| CryptocomError | message |8|code:1.0000000000000002|

The interval31 plus DTO269 positive cases total300; the extra8 constant values and13 retained-unit observations are separate assertions, not additional positive DTO cases. Candlestick string retention does not establish retention after `parseFloat`. These are floors of the maintained types witness, not independent production consumer coverage. Per-row unknown-property omission remains unjustified even where the string representation cause is demonstrated.

## Evidence and review limits

See [SHA-bound gate measurements](DIVERGENCE_AUDIT_GATE_EXITS.md). The new audit test asserts native values before starting its source probe. Two deliberate controls target the native public lookup and the live source table mutation separately; only compiling runtime assertion failures count as controls. All actual exits, including failed harness attempts, are retained in the measurement receipt.

Two read-only sub-agent audits independently inspected the six functions and12 interval/type rows. Their bounded conclusions underpin the classifications above; final adversarial review of the committed annotations and witness is recorded with the gate receipts. No human review gate is requested. No existing finding is represented as a repaired implementation.
