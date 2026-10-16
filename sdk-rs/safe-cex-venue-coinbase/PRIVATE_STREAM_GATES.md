# Coinbase private websocket lifecycle and dispatch

Nine pending→ported rows: connectAndSubscribe, onOpen, onMessage, ping, handlePrivateStreamError, handleUserEvent, handlePositionTopic, subscribe and buildSubscriptionJwt. Authoring parent `536f757500a6eb348752e23387c098e3e9d9e3b0`, a reviewed integration head; no unrelated unreviewed feature ancestor. Full class, order formatter and market resolver remain pending.

Gate SHA `33f21733fe9ea291a7c9309bab4740c381c9f6e2`. Exact commands, measured exits, native test output, controls and census are tracked in `PRIVATE_STREAM_GATE_RESULTS.json`.

- Scoped `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-coinbase --all-targets --jobs 2`: **0**.
- Scoped `cargo test` with `--lib --test private_stream_dispatch --test public_stream_dispatch --test decoder_boundaries --jobs 2`: **0**, 19 tests (9 library, 5 new private, 4 prior public-dispatch, 1 decoder inventory). Oracle environment `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
- Extractor check **0**, text reports zero updated manifests / zero added / zero orphaned. Writer ran after manifest changes and reported one updated manifest, zero added/orphaned.
- Scoped Coinbase invention/status mapping **0**, 426 rows /695 public Rust items. Twenty-seven real native helper/host targets have explicit exemptions, separate from nine real committed public function targets.

## Compiled mutation controls

The endpoint control ran at the gate SHA; payload control ran at corrected-script SHA `224619358a`, remaining four controls at compile-safe script SHA `099e39b97a`, with source/tests diff0 from the gate SHA. Each result records its actual SHA. Each sequential mutation compiled, failed its named native assertion **101**, restored source and passed **0**. Enumeration stopped after six distinct controls /eight mutation attempts (payload-label and compile-safe epoch reruns); no survivors. Initial name-validation mismatch and compile-failed epoch mutation remain in the raw history; neither is promoted to a successful discrimination result. Source/tests restored diff **0**. No broad mutation-census claim.

| Control/call site | Named native assertion | Broken/restored |
| --- | --- | --- |
| connect_and_subscribe endpoint | connect-local: private endpoint: native UTF16 string |101/0|
| build_subscription_jwt to-sign text | open-remote: native to-sign text |101/0|
| subscribe epoch guard | stale/disposed local signature suppresses writes |101/0|
| local Promise adoption versus return-await | local signing adoption source checkpoint write counts |101/0|
| handle_private_stream_error auth predicate | message-auth-error: normal error classification |101/0|
| handle_user_event health mark | message-user: independent source invocation count |101/0|

## Native scope and source proof

Inert typed options with retained root/signer handles, declared string JWT replies, plain JSON error/event records and dense arrays. Host capabilities bind existing routing/local crypto/JSON.parse/BaseWebSocket/reset/health/order-method operations at their source call sites. Native bodies own credential gating, private/tunnel endpoint selection, listener order, attempt increments, sequential signing and guarded sends, failure recovery, message/error/health routing and event iteration. No shared type fields or ExchangeRuntime methods changed.

Signing stays asynchronous: local return-Promise adoption runs a thenable-resolution job and reaction, matching source write counts at three explicit checkpoints (0,0,1). Deferred local signatures produce no writes after epoch change or disposal. Remote synchronous invocation failures reject before microtasks, separately from awaited signer rejection, retaining original failure identity. A late signing rejection closes the current replaced socket even if disposed/stale, after emitting the error; reset remains outside the catch. Native request fields and subscription payloads are asserted directly before any wire operation; accountName None retains undefined, while the native request structure retains the own field. No decode/re-encode comparisons.

The live oracle extracts nine actual bodies, source constants/initializer/reset helper. Content pin `aa8dd526cfb4aae53fdc8d455ecbb1f635a856f77e7c4d936cddf15a992a2382`, independent55-case floor. Six additional controlled source scenarios validate local-adoption timing, epoch/disposal cancellation, current-socket recovery, bounded event iteration/root mutation and synchronous remote rejection-before-marker. Five new native tests compare decoded UTF16/scalars/f64 bits, native payloads/call order and original Rc roots. Health precedes event lookup; event forEach captures original length and dispatches original roots. Auth regex preserves source non-Unicode ASCII case folding and single optional ECMAScript whitespace/API-key separator. Microtasks stop at first quiescence or iteration1000; three checkpoint jobs each stop at their marker or fail at iteration1000; completion inspection stops after two polls.

Imported crypto and JSON parser implementations, full private class assembly, order formatting/market resolution, sparse/exotic/getter/callable DTOs and opaque host exception internals are outside this slice. Local signer async semantics and remote synchronous-throw distinction were corrected after static sub-agent review; final review found no remaining blocker. Reviewer ran no Cargo gates. Precommit ownership/stale-corpus-count and oracle-draft failures are explicitly retained as diagnostics, not attributed to the committed gate SHA.

Tomllib census stops after16 Coinbase manifests /426 rows: **368/17/41 → 377/17/32**, **9 pending→ported, 0→divergent**. Every moved row resolves to its committed file and actual public Rust function. Latest integration-relative census/merge preservation will be recorded before push.

Not run: full Coinbase suite, workspace builds/tests, host clippy, wasm clippy/build, CI, live HTTP/websocket, full class/formatter/market-resolver assembly, imported crypto/JSON parser implementations.


## Integration merge preservation

Actual fetched integration `acf29148c3f26a07b72f93d1d964b49e81016ec6` merged at `15415710111c818ee08e3a1b5824a86227149878`. Sole exemption conflict preserved all958 integration entries plus27 real Coinbase helpers. Source/tests, dependency code/manifests/lock and live venue oracle inputs versus code gate `33f21733fe9ea291a7c9309bab4740c381c9f6e2`: scoped diff **0**. Final oracle/control scripts versus corrected-control SHA `099e39b97a61aa2490de7fa31e5b7abf72ea6b54`: diff **0**.

Scoped check19tests and six compiled controls carry across unchanged inputs; they were not rerun after merging. Merged extractor/mapping remeasured at the actual merge SHA **0/0**, extractor text zero updates, mapping426 rows/695 public items. Direct integration-relative tomllib census: **368/17/41 → 377/17/32**, **9 pending→ported, 0→divergent**, every moved function target verified. `PRIVATE_STREAM_MERGE_GATE_RESULTS.json` binds exact commands, preservation exits and census to this merge SHA. Subsequent delivery commit changes evidence only.
