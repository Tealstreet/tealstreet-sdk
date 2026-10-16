# Encoded pacing and weight-default sub-agent reviews

Pinned read-only reviews; their source-only exits do not become parent native/gate measurements.

---

# Targeted encoded API bridge review

Reviewed committed SHA: `5556653de9f76d8e7edbc57def7530060dfd581c`.
Baseline: `574392f965cd576f04ebf91e47af9e2f2fb00ad7`.

No blocking behavior issue found in the inspected delta. Scope is api.rs encoded request-data handling, the small wait_for_token optional-weight adaptation, tests/rate_encoded.rs and its pinned actual-source oracle. This is not an audit of the whole API/transport or complete limiter scheduling.

## Behavior and boundaries

Encoded UTF-8 request bytes now become Value::String before the pacing classifier, which applies its own parse_request_data exactly once. This fixes the prior double interpretation of a JSON string containing JSON text. In particular, double-encoded userRole is weighted20 instead of60, and a double-encoded exchange batch is not reinterpreted as an action object. Ordinary encoded userRole remains60. The native fixture explicitly prices the existing whole-Value lone-surrogate/depth parser refusal as native20/source60; it does not pretend those cases agree.

The same request_data helper now supplies telemetry with the original raw encoded string, including malformed text and escaped surrogate text. Untransformed values retain their existing value form. Response error routing/cancellation suppression are unchanged; request_action_type already handles encoded text separately. The change does not alter request bytes or replace body provenance in production.

Signing deliberately reparses encoded bytes into the prior serde object view and retains the existing untransformed-provenance refusal. This is separate from pacing: the repair establishes one parse in the pacing classifier path, not one JSON parse over the entire prepare_request operation. The earlier native signing policy itself is not justified as source equivalence by this patch. Invalid UTF-8 encoded inputs, arbitrary transport adapters and transformed/signing composition are outside the finite fixture proof.

The optional-weight API maps None to1 and retains f64 callers. Existing native normalization for nonpositive/nonfinite weights is unchanged. The nine explicit native inputs include omitted/undefined equivalents, positive and negative zero, NaN, negative value, infinity,2 and1.5. This is a typed numeric contract, not generic JavaScript Number/coercion proof.

## Native proof inspection

One test explicitly executes all three native fixture groups before source(): pacing9, telemetry5 and default-weight9. Native expectations use independent literal delays/counts, original data variants/text, exact Host error and provider message, emission/provider counts and returned context field17. The pacing probe makes a second reservation and independently checks clock and sleep values, so a wrong first reservation is observable; it also checks retained Encoded provenance. Negative-zero input bits and NaN input are asserted before execution.

Proof limits: retained body bytes themselves are not independently asserted by the fixture, only the Encoded variant. Telemetry returned context is checked structurally, not by native allocation identity. No transport is invoked: the adapter panics if used, and these are prepare/check_response tests. The source uses supplied capture detection, limiter entry and exchange callbacks around actual maintained interceptors/helpers; it does not construct a full Axios/remote-signing runtime. These are bounded claims, not newly identified production divergences.

## Reviewer measured commands and exits

- `git rev-parse 5556653de9` and `git rev-parse 574392f965`: exit0, exact pins above.
- `git diff --stat 574392f965 5556653de9` and scoped git diff/show reads of the three changed files, actual TS API/limiter and oracle: successful inspections. An initially guessed transport/request.rs blob did not exist; that failed lookup was not a gate or evidence for the conclusion.
- `node` in-memory Module._compile of pinned tests/rate-encoded-source.cjs, redirecting repository script/source reads to git-show blobs at the reviewed SHA: exit0. Output: encoded9 stopping capture (index8), telemetry5 stopping malformed raw text (index4), defaults9 stopping fractional weight1.5 (index8). Source assertions independently check limiter calls8, weights, original config/data identities, provider/emission counts and literal timer expectations. No files were changed by this execution.

Not run by reviewer: Cargo check/test, native baseline failures, mutation/restoration controls, clippy, wasm/workspace build, extractor or formatting. Parent-reported baseline101 and subsequent gates are not reviewer-measured and are not transferred here. No tracked edits, commits or pushes.


---

# Read-only encoded pacing and weight-default review

Review pin:5556653de9f76d8e7edbc57def7530060dfd581c. Comparison parent:574392f965cd576f04ebf91e47af9e2f2fb00ad7. No source edits, Cargo, mutations or pushes. This reviews the three committed changed files and actual source counterparts; it is not a broad venue/API audit.

## Production/default binding

RateLimiter::wait_for_token now takes impl Into<Option<f64>> and binds None to1. Existing f64 call sites remain admitted. The optional binding occurs before the same boxed async body; first poll, FIFO ticket insertion, retained reservation, front sleeper/queued follower drop and lazy unpolled behaviour are byte-unchanged. This does not claim their existing CHOSEN Promise/cancellation divergence is repaired. Source number=1 default and native None share the omitted/undefined boundary; Some0,-0,NaN,negative and Infinity retain the existing normalized1 path. A generic caller's custom Into conversion is native argument adaptation, not a JavaScript coercion implementation.

Nine native reservation rows include two None cases corresponding to source omitted/undefined, explicit0/-0/NaN/-2/Infinity/2/1.5. They independently expect50,50,50,50,50,50,50,100,75ms, check negative-zero input bits/NaN before use, and observe the next reservation and sleep list before any source process. Two None rows are not distinct native runtime values; the oracle separately exercises the two source calling forms. No native execution was performed by this reviewer.

## Encoded API and telemetry composition

request_data now returns original UTF8 encoded text as Value::String instead of pre-parsing JSON. The limiter's existing parser therefore parses once at the source entry boundary. Double-encoded userRole remains a String after that single parse and yields20; the old preparse plus classifier parse incorrectly yielded60. Direct object-text yields60. Exact emitted original request-data variant is checked directly at the typed telemetry callback before source, including double-encoded and malformed text. These checks establish native value/variant correspondence, not JavaScript object heap identity: Value/callback ownership remains a separately priced snapshot API.

Signing keeps its separate fail-closed Encoded provenance handling after pacing. Explaining that existing refusal is not an additional behaviour repair in this four-row selection. The temporary signing parse is not used to feed pacing/telemetry again.

## Planned four-row classification

Supported proposed split:3behaviour/1explanation, scoped to the selected four source rows, not distinct causes or all FINDING text:

- getOrderErrorTelemetry: behaviour; reparsing encoded bytes before callback lost the original raw String/value variant. Native/source five-case floor ends malformed raw text.
- addRateLimitInterceptor: behaviour; double parsing changed double-encoded userRole from source20 to native60. Native/source nine-case floor ends capture. The raw-text fix and telemetry fix share request_data but remain two source symbols.
- RateLimiter.waitForToken: behaviour; missing default input binding is now an actual callable Option boundary with nine independent numeric/default expectations.
- getHyperliquidRestRequestWeight: explanation only if the manifest explicitly retains and prices the existing whole-Cow<Value> parser restriction. Source parses ignored U+D800/depth140 payloads as userRole and yields60 while native leaves the original String and yields20. A selected-field decoder is feasible; retaining the shared whole-value parser avoids a separate venue decoding contract and preserves its public Cow<Value> result domain. Name the practical caller cost: default pacing reserves1000ms versus source3000ms for the measured examples. A UTF8 label alone would not justify refusing otherwise relevant fields. This is not a repaired parser path; classify the known representational/parser decision honestly under MANIFEST-SPEC#divergence-taxonomy, with #shared-utf8-string-boundary/#forced-surrogate-conflict only where their conditions actually apply. No universal raw-JSON or parser-depth parity claim.

The same parser cost was already measured in earlier source/native fixtures; this is documentation of an existing cause, not a newly discovered native failure. Four-row ratio should explicitly say3repaired behaviours and1documented retained choice, without counting the signing note clarification or implying all parser findings closed.

## Measured reviewer execution

Pinned source oracle exit0 at this exact SHA. The oracle and four TypeScript dependencies (api,rateLimiter,types,order-response) were read from committed git blobs through a read-only loader, so concurrent worktree mutations were excluded. Output: encoded9 ending capture; telemetry5 ending malformed raw text; defaults9 ending fractional weight1.5. Assertions run in actual extracted source bodies/full RateLimiter class with controlled virtual clock and timer bindings.

Git show/diff and source inspection completed0. NOT RUN: Rust/native tests, Cargo check, host/wasm Clippy, workspace builds, extractor, fmt, native/source mutation controls, cancellation/drop regression targets. Parent-owned exits are not transferred into this review. No concrete new production defect identified within the stated optional-weight/encoded-value slice; conditional taxonomy qualifications above remain for the planned manifest edit.


---

# Narrow pacing metadata follow-up

Exact review pin:fe47dff911a3ddd90334844bbb04719811f41fb0. Read-only committed git-show/diff and Python3.11 tomllib checks; no Cargo, edits, mutations, new audits or pushes.

The commit changes notes only on the four audited rows getOrderErrorTelemetry/addRateLimitInterceptor/getHyperliquidRestRequestWeight/RateLimiter.waitForToken, plus the separately disclosed addSigningInterceptor note. Row identity, targets, statuses and counts are unchanged; no status movement is claimed. The added signing explanation is explicitly excluded from the new four-row repair count.

The parent classifier now states the actual retained cost: source userRole60/native20 for ignored source-materialized D800/depth140; at default50ms/token the subsequent reservation is3000ms/source versus1000ms/native. It names whole Cow<Value>/RestBody parser reuse and the avoided second selective UTF16 JSON/coercion/depth model, rather than claiming UTF8 makes the loss unavoidable or that this parser behaviour was repaired. The actual encoded prepare_request entry and direct helper are separately identified. This resolves the prior conditional rationale qualification.

The rate-interceptor note separately names the repaired doubleparse cases: quoted role20 instead of60 and quoted order batch1 instead of2; capture bypass and lazy/error ownership remain priced. Telemetry accurately claims original native String variant/content for validUTF8 Encoded input, and separately prices owned JSON callback/context identity and lost live lookup/error protocol. Signing explicitly discloses source String skip versus existing native opaque Encoded fail-closed refusal, including key validation precedence and proven Transformed String skip. It does not call this existing refusal another behaviour repair.

Optional-weight note accurately states f64/Option<f64> binding, None default1, explicit zero/negativezero/NaN/negative/infinity normalization and9case finite floor ending1.5. It retains lazy/unpolled/drop ownership differences rather than claiming Promise/Date/timer equivalence. Supported four-row split remains3behaviour/1explanation, with the signing rationale clarification outside that denominator. No remaining blocker found within this metadata-only follow-up.

Scoped diff from5556653de9f76d8e7edbc57def7530060dfd581c finds no source/test change; only scripts/rate-ingress-controls.py differs among venue source/tests/scripts. Driver controls were not executed or reviewed as new gates here. Reviewer's earlier pinned source oracle at555 exited0 (9encoded/5telemetry/9defaults); it is not restated as a newly executed gate atfe47. Parent-measured compiled controls/restores are not reviewer exits.

Measured here: git pin/show/diff and final note-only/status-preservation tomllib check exit0. NOT RUN: Cargo/native tests, source runtime, mutation controls, check/Clippy/wasm/workspace builds, extractor/fmt. No whole API/JSON/accepted-set audit claim.

