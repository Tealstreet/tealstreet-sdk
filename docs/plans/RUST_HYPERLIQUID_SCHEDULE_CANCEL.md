# Hyperliquid scheduleCancel parity

Clock policy supersession: [engine clock standardisation](RUST_ENGINE_CLOCK.md)
replaces the historical per-venue `hostClock` opt-in below. Production SDK
engines share one synchronized default; explicit host/tape clocks remain
authoritative. Original implementation/regression receipts are retained.

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

Plan audit refinement before implementation:
- HTTP408 is Unknown even with a well-formed native refusal, as are5xx; a
  structured err/string below500 otherwise proves Rejected. Pin this precedence.
- Reuse nonce sharing only within its existing native-process/WASM-module
  boundary; no new cross-worker/process coordinator is implied.
- Handle schedule replies before generic trading reconciliation so order reads
  cannot imply arming or mutate settings; malformed/unknown ACKs fail closed.

Implementation phase evidence (before current landing integration):
- Original unsupported JSON/capability/control regression: seven groups fail
  before implementation; eleven native schedule groups and ordinary-write queue
  regression now pass. Hyperliquid + Binance: 431 tests pass.
- Eight independent TS signer vectors pin exact MessagePack, action hash and
  signature for both networks, vault/no vault and arm/clear. Rust verifier uses
  typed fixture records; no runtime JSON-value signer path is introduced.
- SDK: 31 focused tests pass, including invalid values before worker/JSON,
  synchronized native command time, void completion, worker route, native
  uncertainty and capability refusal. Typecheck and explicit ESLint pass.
- Host: 87 command/diagnostic/wire tests pass. Independent review exposed direct
  host NaN -> JSON null -> clear; the original host fails the new zero-dispatch
  regression, and the narrow safe-integer ingress fix passes.
- Default combined-host capability fixture passes. Strict touched-crate Clippy
  passes; native and SDK/host independent reviews are clear after fixes.

Remaining integration phase: actual shared landing + #1689; preserve Backpack
and MEXC precise decoder additions, add schedule probe alongside the family book
probe, refresh focused checks and exact-head CI before any merge. No API closure
or caller deployment is inferred from a draft PR.

Integration phase:
- Genuinely merged actual landing `3db712f04466` (Backpack #1680, MEXC #1681
  and Hyperliquid harness #1689). Bounded docs/appended-test conflicts preserve
  both incoming closures and Backpack regression; existing precise book Cargo
  feature/decoder and family probe stay intact.
- Added the capability-driven schedule JSON ingress probe to isolated family
  artifacts, alongside the book probe. It constructs public-only engines and
  never executes returned transport actions; CI supplies built artifacts.
- Refreshed 431 native tests, 47 SDK/worker tests, 87 host/wire tests and the
  combined-host capability fixture pass; typecheck and explicit ESLint pass.
  Final independent adversarial integration review is clear. Full exact-head CI
  and a fresh separate guard remain required. The Orderly #1692 priority window
  holds source merges until the overseer releases it.
