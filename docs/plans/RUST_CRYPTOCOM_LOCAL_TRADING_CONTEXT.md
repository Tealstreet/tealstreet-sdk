# Crypto.com immediate trading command custody

Plan first, codex-ih35js, 2026-10-24. Exact landing:
`9330c11bdea731368cb1254fa3dc068d11380cdd` (#1788 landed).
Read with RUST_CRYPTOCOM_REQUEST_CONTEXT.md, RUST_REQUEST_CONTEXT.md and the
caller/audit/quality ledgers. This checkpoint plans and tests; it changes no
production, existing fixture, Cargo, checker or manifest byte.

## Exact inventory and slice

Current Crypto.com inventory is **20 path/fingerprint entries /21 occurrences**:
account2/2, history4/5, identity2/2, lib1/1, trading11/11.
This slice removes **8 trading fingerprints /8 occurrences**, leaving trading3/3
and venue**12/13**. It is the largest immediate independent subset: seven local
refusals and one fixed-setting success. No open PR in the file-list audit touches
Crypto.com source or tests. #1788's account/trading hold ended on this base.
WOO belongs to codex-5c57jl; MEXC public work is held by our #2034 and private
signers remain coordinated separately; Aster's local work remains in #1987/#2027.

| Path under venues/cryptocom/src | Exact fingerprint | Count | Destination |
| --- | --- | ---: | --- |
| `account.rs` | `83c6b712d19379913a85bf575a1a1834358d1b5f84d4c7a04a2f241487399a9c` | 1 | Retain: shared signed REST funnel or conservative witness; managed signer/account adoption |
| `account.rs` | `a77d14858a6b097716fd945cddafa2c827e671194bbe1122e95c60b698c56006` | 1 | Retain: shared signed REST funnel or conservative witness; managed signer/account adoption |
| `history.rs` | `073a9daacf5c0553bfb2650ddc8db7b942b5f6a0c8125261daff7479814953b7` | 1 | Retain: native history send/retry/continuation; managed immutable root and shared scheduling |
| `history.rs` | `7698d5bd23a40574165507b4b54b5d093fb00701512d29250192f1345e4da553` | 1 | Retain: native history send/retry/continuation; managed immutable root and shared scheduling |
| `history.rs` | `d702253b3d082dc8c7278bfe17bdb876eeda775fedd004ebaaeea6cdd16f24ad` | 2 | Retain: native history send/retry/continuation; managed immutable root and shared scheduling |
| `history.rs` | `dfb17bdf89f16befd8bb7566ac05d0bddf2980c6627b642f20e1f1b5a4859508` | 1 | Retain: native history send/retry/continuation; managed immutable root and shared scheduling |
| `identity.rs` | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | Retain: shared identity terminal; signed retrying Read adoption |
| `identity.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | Retain: shared identity terminal; signed retrying Read adoption |
| `lib.rs` | `121dc8add2c9a7d1d8fd691023a64c9ebb736360f0c97363cfc66956b0ec1405` | 1 | Retain: public discovery sender; separate Background Read adoption |
| `trading.rs` | `2b3480974c11d493d8859f3ed1ecc2ed3370363182f22e5394ca7b6ccfedb502` | 1 | Invalid leverage or market → actual Mutation entry/refusal |
| `trading.rs` | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 1 | Fixed position-mode success → actual fixed Setting completion |
| `trading.rs` | `5bfc783b8505466a5fad6b439e8145ed6557a4b2bd22a1b4d7f66e531c80206b` | 1 | Missing reconciled cancel target → actual Mutation entry/refusal |
| `trading.rs` | `6c857f43df6f903845b498ba957ba70aedefef5068f0c26206cfd87745bd4c2a` | 1 | Missing amend target → actual Mutation entry/refusal |
| `trading.rs` | `7707e9a5c95ee3fc74e58a5fb683b90895a4a030f729a56f55ae1752cd3a1a5f` | 1 | Unsupported margin switch → actual Mutation entry/refusal |
| `trading.rs` | `8c1d78bb768fb392694ad91c49f963ab64ba87a71d151646bff0298b85a6b734` | 1 | Retain: remote/empty-job terminal or conservative request macro; full managed write adoption |
| `trading.rs` | `acacfde7f264666789ccf383262670eebe6988b188d3747f0388d825e3072f6d` | 1 | Missing primary credentials → actual Mutation entry/refusal |
| `trading.rs` | `ae3209100adb7617d3a60abc05d13961f6b17901cc02551743ff77b75e18b37f` | 1 | Placement-plan refusal → actual Mutation entry/refusal |
| `trading.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | Retain: remote/empty-job terminal or conservative request macro; full managed write adoption |
| `trading.rs` | `f32eb32f491028d71654d8ec05bbdf592328cd7f7156d848a17d430553a77638` | 1 | Retain: remote/empty-job terminal or conservative request macro; full managed write adoption |
| `trading.rs` | `f577ab41ba475c27e97cb5fb09a7c4216a6b20a88defe331a0dc268f15dfc531` | 1 | Invalid amend price or amount → actual Mutation entry/refusal |

## Semantic authority and native precedence

All seven refusals are **Mutation**, including invalid leverage before its
semantic account-read preflight. Use one direct genuine
`cx.admit_http_command(Policy { operation: Mutation, credential: None,
timeout_ms: 20_000, pacing: None, retry: Never })` at the local terminal boundary,
then `fail_context` on that exact root. Rejected already emitted one failure;
Revoked grants none. Never add a numeric fallback, a local issuer wrapper, an
unsigned HTTP request, a journal fact for an unsent leg, or a Read relabel.
Credential-free local validation has no credential-health evidence even when the
native refusal kind is AuthInvalid. This uses actual landed APIs on this base.

Compute local planning without mutating Store, the trading queue, active job,
progress receipts or timers. Preserve these original precedence rules:

- Cancel checks every id before any credential check or write; mixed known and
  unknown ids refuse atomically with the first missing native id.
- Amend missing target precedes amount/price checks, then credentials. Preserve
  positive finite fallback to stored price/amount and the advanced-order route.
- Leverage rejects missing market, non-integral or outside1..100 before checking
  credentials. Margin switching always refuses Unsupported before credentials.
- Placement computes a pure plan first, then missing primary credentials masks
  plan errors. With a primary key, the original plan error emits command_error
  followed by failure; emit that notice only after successful local admission.
- Preserve exact error messages, kinds and NotSent outcomes; no new local code
  table, status ladder or auth inference.

`SetPositionMode` is a fixed native **Setting**, independently of credentials,
requested hedged value and symbol. Use actual `cx.done_local_setting("[]", |_| {})`
with no Context/HTTP or new timer/journal. This preserves the native no-op Store
projection (it does not rewrite Store.hedged or settings). Its warning is emitted
only after Ok. The existing API publishes its terminal before returning, so the
warning follows success instead of preceding it; denied/revoked admission emits
no native warning. Pin that admission-dependent ordering explicitly; do not
invent an engine extension or pre-check receipt/capacity locally to retain order.

Valid remote plans issue no local Context and continue through the unchanged
native job, signed wire, progress and pacing path. Empty PlaceOrders/CancelOrders
jobs still finish through the shared legacy drive_trading terminal; those two
terminal fingerprints are deliberately outside this8-site slice. Do not claim
all synchronous trading paths are managed. Remote error40101 resend, best-effort
aggregation, typed write classification and snapshot reducers remain separate.

## Caller effects and bounds

Immediate local refusals and the fixed-setting ack now require genuine fresh
command custody. Invalid clocks/missing receipts, full256-owner capacity,
duplicate live ownership and expired/sub-ms caller waits cannot publish the native
ack/refusal, warning, command_error or financial state. Shared admission supplies
its typed cause once (Mutation NotSent); do not overwrite it with a business
message. Finite admitted business errors and fixed success keep their native
payloads and current Store. The successful warning/terminal ordering above is
the only additional sequencing adjustment. Missing-key failures no longer count
as remote credential evidence. No caller arguments or signed bytes change.

The implementation PR must append these effects to CALLERS. Existing native
financial tests remain unchanged; no signer, account, history or lib callback
region is needed. No engine PR or local release build is a prerequisite.

## Original controls before production

Commit a dedicated real-Engine integration target separately from this plan.
Use the already-managed public book read to occupy genuine peer owners; no
fabricated command id, credential, physical receipt or completion is allowed.

- Pin all seven finite native refusals, both key precedence orders, atomic mixed
  cancel batch and unchanged Store, including placement command_error ordering.
- RED each selected refusal under non-finite clock and full owner capacity;
  one engine kind/NotSent terminal replaces the native business projection.
- Free one real peer, re-use the refused id finitely and get the original native
  result; every other peer stays live. Repeated local refusals do not leak slots.
- Pin keyless fixed position-mode finite success/no Store change/warning, then
  RED invalid clock, capacity and expired/sub-ms held caller wait. Denied entry
  emits no native warning; successful admitted warning follows the terminal.
- Duplicate a live managed book id with a local refusal/fixed ack; the existing
  read stays live and settles once without cross-command completion.
- Working controls retain valid private placement wire/progress and empty-job
  completion; these do not count as managed remote write adoption.
- Original REDs must execute assertion failures on unchanged base production,
  with working positive controls, no ignored tests and strict scoped test lint.
  Keep original assertion bodies identifiable during later adoption.

Independent source/adversarial, caller and full-map reviews precede production.
Only plan/new test/index changes at this checkpoint. Local checks follow the
current load ruling: one build at a time, CARGO_BUILD_JOBS=4, only the changed
Crypto.com target and its scoped lint; no all-feature/workspace sweeps. CI owns
WASM size/CPU/memory. No merge without the overseer's explicit window.

Architecture gate: **No**. Native codecs adopt landed genuine Mutation admission
and fixed Setting completion; they implement no per-venue lifetime, issuer,
retry, signer, journal, classifier or finalizer mechanism.
