# Bitget native dispatch signer adoption

Plan, 2026-10-22. Fresh actual base
`5f6488c7664b68cf8b866d2f7f914a3fc99e5fc6`; no OKX or unmerged half-2
source is carried. Keep #1757/#1737/landing diagnostic watches active. This
checkpoint is native-only and cannot merge as hosted-compatible before the
separate shared encoding/passphrase presentation extension lands.

1. Replace Bitget Options raw key/secret/passphrase and secondary ReadCredentials
   with opaque Identity refs. Consume legacy input once through the existing
   trusted setup provider. Use #1738's primary required-for-private passphrase
   and secondary required-with-primary-fallback: absent inherits, explicit empty
   refuses. Incomplete primary credentials retain existing public construction;
   incomplete secondary refuses before startup. Native and WASM Bitget feature
   gates must select the shared dispatch-signing provider without Binance.
2. Move actual private account_request, trading issue_trade, history send and
   fetch_identity builders to SignStep. Keep public get/candles/history unsigned,
   exact body/path/query/header ordering, client IDs, UTA/Classic/product routing,
   streamed response ownership and native compound trade planning unchanged.
   Ordinary private GET selects secondary; degraded balance recovery GET is the
   existing primary exception and must keep RequestRoute::Primary. POST and WS
   are primary. Preserve the current resolve-amend GET identity selection.
3. HTTP work is shared provider HMAC-SHA256 over integer protocol-ms timestamp +
   uppercase method + exact path/query + exact body, Base64(raw MAC). Atomic
   Present key + MAC + Present passphrase; preserve ACCESS-KEY,
   ACCESS-PASSPHRASE, ACCESS-SIGN, ACCESS-TIMESTAMP, broker and Content-Type order.
   Use DispatchContext.stamp_ms(), no venue clock or retained signed cache.
4. Private Classic/UTA login uses send_signed primary work over seconds +
   GET/user/verify. Preserve string timestamp, JSON field order/escaping,
   login-before-ping FIFO, socket supervision, native refusal classification and
   raw scheduling time. Shared signer freshness/retirement fences own delayed
   completion; no new WS state machine or retry policy in this family port.
5. Preserve owned typed failures. First unsent write is notSent; previously sent
   attempts cannot become wholly notSent on signer/provider failure. Reuse the
   existing job partial/effective/unknown evidence and signature-retry flag,
   report a missing shared aggregation seam instead of adding a local ledger.
   History and lookup signing refusal stay strict, never successful empty/absent.
   Background failures release only current-generation latches and use existing
   backoff; provider failure is not native auth rejection or UTA feature evidence.
6. Retain the old signer only as a cfg(test) independent oracle. Add real-builder
   provider refusal, delayed reprepare/late duplicate, both credential slots,
   degraded-primary balance, Classic/UTA WS and three-alias registry tests.
   Preserve existing financial/history/socket tests via test-only trusted setup.
   Include an OKX-style runtime RED control for typed owner handling. Expand
   signing-module scan to Bitget and shrink allowance exactly; no unused entries,
   production crypto/secret custody or broad scan exclusion.
7. Add same-PR RUST_CUTOVER_CALLERS and Bitget CLAUDE entries for provider/worker
   provisioning, absent-only passphrase fallback, routing/readiness and draft
   hosted boundaries. Shared remote presentation plan lives separately; hosted
   admission/standalone request shaping and all-alias actual-WASM HTTP/WS positive
   tests must follow that actual dependency, not SDK secret restoration/new wire.
8. Run focused Bitget+Binance/engine/registry checks, strict fmt/Clippy and SDK
   type/lint/setup tests; independent correctness/adversarial reviews per phase.
   Final full CI, complete native signature/financial replay comparison, actual
   shared-input integration and own-only separate guard precede any merge under
   owner order. No local release/replay/size/timing run.

Architecture gate: this is adoption of the existing engine signer/provider and
shared setup policy, not a per-venue fix of a cross-cutting concern. Shared
encoding/passphrase presentation is already peeled and approved for OKX/Bitget;
RequestContext owns physical release/pacing/outcome consolidation. This draft
adds no scheduler, provider queue, clock offset, retries or outcome ledger.

## Native checkpoint implementation and evidence

Actual account, identity, history and trade HTTP builders plus Classic/UTA login
now stage shared SignStep work using opaque identities. Public builders remain
unsigned. Test fixtures retain their raw test-input shape only in test helpers,
where the same #1738 trusted setup owns the provider. Existing financial, history,
mode, row-admission and socket assertions remain; required missing/empty material
now asserts synchronous setup rejection rather than a later startup callback.

Nine real-builder tests cover Probe refusal without Classic promotion; strict
identity/history refusal; metadata refusal without transport retry; first-write
notSent and duplicate completion; native40009 prior-attempt Unknown; rollback
original native error detail; delayed timestamp reprepare retaining intent and
primary route; and private WS FIFO/retired completion. A review finding reproduced
runtime RED when rollback refusal dropped nativecode40017; reusing the existing
original-based error projection fixes it. Bypassing owned signing-error handling
also reproduces six runtime RED assertions, restored source passes all nine.
Three aliases independently exercise native HTTP secondary/fallback and private
WS primary MACs; standalone Bitget-only provider construction passes.

The scan now includes Bitget signing modules with exactly501 allowances/matches
from517 on this base. All non-Bitget entries are unchanged. Production has two
shared work-builder descriptor matches; remaining Bitget sites are cfg(test)
setup/oracles. Independent correctness and adversarial reviews classify this as
shared signer adoption, not new cross-cutting policy. No hosted readiness or
actual-WASM/replay claim is made. Those prerequisites and the owner queue still
block readiness/merge; this checkpoint is local, reviewable native progress.

Checkpoint gates: 310 Bitget/registry native tests and266 Binance tests pass,
including nine real-builder provider tests and standalone Bitget-only alias
construction. Strict all-target Bitget/registry Clippy, fmt, SDK typecheck/lint
and59 focused SDK setup tests pass. Independent adversarial scan verifies exact
501 matches/501 allowances and unchanged non-Bitget allowances. Reviews are
clear after the rollback fix. Final actual-WASM/replay/hosted gates are pending.

## Lane takeover status (claude-4yjlw0, 2026-10-23)

Checkpoint `686cf48c92e` confirmed: native-only, 2 commits ahead and 457 behind
landing `73d04380dd9`, no PR. A copy of the TS #8093 diff
(`123d14f64a7`) is kept at `.aimux/plans/bitget-next-8093.diff` in the main
checkout because the `/tmp` copy is not durable.

Next, sequenced after #1773 because both edit hosted registry admission:
1. Merge pinned landing, including actual #1839 and #1773.
2. Hosted admission per alias with actual-WASM HTTP/WS positives. Generalise the
   shared WS-seconds codec to construction-declared suffix data and move OKX to
   it in the same PR. Keep the native three-part shape and exact receipt.
3. Reverify the #1814 rows (Bitget 16/34: partial IDs, typed auth, f64 codes,
   40009 resend) through shared tables. Then open a draft PR.
4. UTA routing parity, as a separate follow-up PR: wsPrivateUta/privateUta
   route fields, proxyConditional `/bitget/v2/private` and `/bitget/v3/private`,
   relayUpstream/relayWhenTunnelled, the immediate 40018 auth-refusal walk and
   native prelogin/connect fallback bounds. Replace the routing test that
   requires UTA to stay direct.

## Hosted admission status (2026-10-23, uncommitted slice)

Step 2 of the takeover sequence is implemented on top of merged #1773:
1. All three aliases are admitted to registry hosted capture and declared
   keyless RemoteSigner (`payloadSupported: false`). Hosted HTTP uses the new
   standalone `bitget.signing.ts` codec, also used by the TS interceptor.
2. The WS-seconds codec is generalised to `secondsTimestamp` plus a declared
   `secondsSuffix`; OKX moved to it in the same change with unchanged OKX
   expectations. Bitget native login now emits the shared three-part work.
3. RED-first tests: 25 TS and 6 Rust assertions failed before the fix and pass
   after; per-alias actual-WASM HTTP/WS/native-login tests are authored for CI
   and were not run locally (no local `wasm-bindgen`).
4. Two stale pre-merge assertions that Bitget was unmigrated were retargeted
   or removed; four unused OKX dispatch signing allowances were shrunk exactly.

Remaining: CI actual-WASM and replay evidence, #1814 row reverification, the
request-context inventory decision, then the draft PR. UTA routing parity stays
a separate follow-up.
