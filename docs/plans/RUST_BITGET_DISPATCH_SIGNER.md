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
