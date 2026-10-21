# Hyperliquid scheduleCancel parity

Fresh actual landing: `d510b9813235f7bf99c6eac9c630a583fb0c1212`.
Plan first; independent of book-query implementations. No source merge during
Hyperliquid harness #1689 priority window. Integrate its actual pin and current
shared source before final CI, preserving exact diagnostics/tape evidence.

1. Reproduce absent SDK/native capability and standalone JSON admission. Add a
   precise command-schedule-cancel decoder feature mapping scheduleCancel to
   VenueAction::ScheduleCancel, enabled by Hyperliquid only. Append a boolean
   scheduleCancel capability at bit25, leaving existing flag/enum positions
   unchanged; only perpetuals advertise it. Standalone spot/public-only refuse.
2. Add native signed arm/clear through the existing sequenced334ms write pipeline,
   no market seed, builder lookup, order identity or local order mutation. Null/
   omitted time removes time from JSON AND MessagePack. Supplied time must be a
   finite safe integer UTCms and at least cx.now+5000 at admission and again
   immediately before signing/dispatch; queued expiry is NotSent and frees queue.
   Convert to u64 before signing; preserve type/time field order in named msgpack.
3. Reuse existing L1 phantom-agent signer, globally monotonic signer nonce and
   vault-aware digest/payload on live/testnet. No expiresAfter or automatic retry,
   rearm, heartbeat, startup/disposal clear, schedule-state flag or account read
   claiming arming. Enable existing synchronized hostClock for perp deadlines and
   nonces; prove SDK clock skew/control. Spot clock/capability remains unchanged.
4. Use strict object ACK validation scoped to schedule effects, avoiding existing
   permissive named-struct sequences and financial/order reconciliation paths.
   Reuse conservative control type=default with empty statuses as an inference
   from the official SDK control response, not a captured schedule ACK. Explicit
   status err/string proves Rejected only below5xx; transport/timeout/5xx/bad ACK
   is Unknown after dispatch. Preflight/signing failures are typed NotSent. No
   replay/resend; other queued jobs progress and no account/schedule mutation.
5. Override RustEngineExchange.scheduleCancel as Promise<void>, gating the native
   capability and existing typed worker route. Validate JS type/safe integer/time
   BEFORE JSON or worker serialization (NaN/Infinity must never turn into null
   and clear). Await owner completion and discard native payload. Reuse existing
   worker method/write ownership; add EngineHost WRITE_COMMANDS interruption
   classification so timeout/dispose/transport loss remains Unknown.
6. Regress original unsupported cap/JSON/method; arm/omitted/null clear exact
   JSON/msgpack/hash/signature vectors main/test x vault/no vault; safe integer/
   5s edges and stale queued deadlines; shared nonce/ordinary write sequencing;
   invalid signer/vault no HTTP; strict ACK/native refusal/HTTP precedence and
   interrupted/duplicate command ownership; spot/public-only/no startup/dispose
   effects; SDK/worker void/errors/clock; append-only wire bits and isolated family
   decoder probe for the advertised capability. Preserve B11/NX caller contracts.
7. Independent plan and implementation audits per phase. Focused Hyperliquid+
   Binance and touched engine/host gates, SDK tests/typecheck/lint, strict Clippy/
   fmt/bans and JS host tests; full exact-head CI (including family artifacts and
   replay outputs), real current shared landing integration and separate exact
   guard before actual merge verification/overseer+caller receipts.

Primary contract: [official schedule documentation](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/exchange-endpoint#schedule-cancel-dead-mans-switch)
and official Python/Rust SDK L1 action/signing records. Native trigger count10 per
UTC day and caller-owned renewal stay venue/caller policy; no automatic lifecycle.
No live actions/captures, NX edits, budgets/allowance widening, release automation
or local release/full replay/timing builds. Four local Cargo jobs.
