# ApeX Omni RequestContext inventory and migration plan

2026-10-23, lane `codex-hcmq3f`, overseer `claude-ddi2il`.
Base: actual landing `7641650e7b1fc54c55593880ac16299171f60a90`.
This is the plan for removing every ApeX entry from the legacy request ratchet;
it does not claim managed adoption, CI proof, live connectivity or cutover.

## Scope and collision audit

First venue: ApeX Omni USDT/USDC perpetuals, crate
`sdk-rs/venues/apexomni`. Its unsigned candle read is the first implementation
area. Keep the venue's public/private financial mapping, native IDs, virtual
brackets, zkLink bytes, supported products and capability limits.

Before edits, `gh pr list --base feat/rust-rewrite --state open --limit 100
--json number,title,files` returned 21 open PRs. No ApeX source or test path is
occupied. Within this lane's other venues, defer these occupied files:

| PR | Occupied source/test area |
| --- | --- |
| #1798 | Kraken `src/spot/book.rs`, `tests/spot_book_crc32.rs` |
| #1797 | Kraken `src/spot/trading.rs`, `tests/spot_trading.rs`; BreakoutProp `src/history.rs`, `tests/history.rs` |
| #1799 | Coinbase `src/lib.rs`, `tests/support/history_family.rs` |
| #1787 | Backpack `src/account.rs`, `src/trading.rs`, `tests/account.rs`, `tests/trading.rs` |
| #1791 | Orderly `src/history.rs`, `tests/history.rs` |
| #1794 | TxFlow `src/history.rs`, `tests/history.rs` |
| #1785–#1788 | TxFlow `src/account.rs`, `tests/account.rs` |
| #1800 | Polymarket `src/lib.rs`, `tests/history.rs` |
| #1786 | Polymarket `src/account.rs`, `tests/account.rs` |

The overseer's same-day correction permits shared-by-design inventory and plan
appendices despite overlap. For integration, keep incoming docs first and this
appendix once verbatim; preserve incoming inventory and reapply only ApeX
removals. Recheck open PR file lists before each source/test slice. The manifest
may only shrink; no exemptions, changed-call rehashes or scanner relaxations.

## Engine destinations

Use the landed `request_context::{Context, Lifecycle, Policy}` and `Cx` APIs.
At the base above, `Call` is not a public engine type. The shared plan-admission
owner `codex-uufub4` confirms that its prospective `Call`/`Admission` stay
private; its proposed preflight/Plan interfaces are local and not dependencies
until reviewed and landed. Never reproduce them inside this venue.

The inventory below maps every fingerprint/count to one of these paths:

| Path | Engine adoption and boundary |
| --- | --- |
| CANDLE | One command-owned public `Operation::Read`, no credential, `Cx.request_managed`, exact original deadline, `validate_read_http_delivery(root, leg)` before data/diagnostic publication, `done_context`/`fail_context` once. Keep native interval, query and row-validation facts. |
| IDENTITY | One signed read using the host-signer lane's recipe and explicit credential reference. Retain original command authority and the exact delivered leg; validate before identity publication and finalize contextually. |
| BACKGROUND | Explicit background read contexts for `/v3/symbols` discovery and standalone account/balance/order reads, shared scheduler/release admission, then `finish_http_context` before replacement work. Keep native pending flags, coverage and financial snapshot fences; joined snapshot ownership awaits the shared plan/group seam. |
| HISTORY | Existing `HistoryRows` staging/accepted rows stays authoritative. Adopt shared context-bearing Completion plus the reviewed plan for native/display alias and fills/history-orders alternatives. One original root/deadline spans all physical attempts; no numeric completion bypass. |
| WRITE | Shared preflight/Plan admission, managed signed mutation legs, existing bounded reconciliation hook, `http_write_result` native facts and central context finalization. Remove local `Job.effective` outcome aggregation; keep only native sequencing/stop facts. |
| LOCAL | Reviewed shared no-I/O completion: read-only `done_preflight` or explicit `done_empty_plan`, and validated preflight rejection. These APIs are not landed at this pin. Do not mint dummy Contexts/timers/accepted facts or retain numeric finalization as the zero-inventory solution. |
| SIGNER | ApeX is outside `codex-5c57jl`'s eleven-family queue. Ask the overseer to assign its signer adoption before this slice. Eager `sign::request` must become an unsigned recipe consumed by the shared dispatcher, preserving primary/legacy canonical bytes and credential routing. zkLink contract packing is a distinct native payload fact. |
| LEXICAL | Scanner conservatively counts an enclosing macro containing the variable token `request`; there is no legacy API invocation here. When that codec is legitimately migrated, rename its native descriptor binding or remove obsolete codec code, prove identical canonical bytes, and prune only the exact vanished fingerprint. No scanner changes or exemption. |

Row admission stays in engine coverage helpers. Account streams already use
`SnapshotBuilder`; migration must retain rejected-row copies and native version,
terminal, hidden-order and fill-observation fences. Any missing common row or
snapshot admission belongs to `codex-mxb4mq`, not a new ApeX policy.

## Committable order

1. Commit this complete inventory and audited plan, then report its commit and
   collision map to the overseer before implementation.
2. **Unsigned candles only.** Commit original-source failing controls first in
   `sdk-rs/venues/apexomni/tests/request_context_candles.rs`. Then migrate the
   candle continuation in `src/public.rs` and narrowly route it in `src/lib.rs`.
   Update only required crate fixture drivers using engine `prepared_actions`;
   preserve every original endpoint, result, clock and Store assertion. Leave
   shared eager signing and all non-candle branches unchanged in this PR.
3. **Single private reads.** After explicit signer ownership and a landed recipe,
   migrate identity and genuinely standalone background reads in separate small
   PRs. Do not split an atomic joined snapshot into apparently independent
   publication owners. Discovery through `/v3/symbols` currently uses the eager
   signing wrapper even without private credentials; coordinate its dispatch
   declaration rather than silently dropping its current wire headers. Any
   background authentication/session mutation needs the future shared Session
   Context; it cannot be labeled Read or borrowed from a command.
4. **History and joined snapshots.** Wait for actual shared plan/Completion
   adoption. Inventory all aliases/stages in one admission; retain the existing
   successful REST/cache selection and all-actual-failure typed rejection.
   Multi-owner/coalesced observer completion stays last until its shared API is
   ready. Keep the current atomic positions/orders/metadata publication; it
   marks the account initialized before requesting balance independently. A
   later failed balance read cannot roll back that already committed pair.
5. **Trading and local completion.** After actual plan/signing prerequisites,
   migrate single-write commands, then split placements/cancel batches and
   cancellation/replacement. Preflight rejects before any write; each physical
   mutation reports native acceptance/refusal/unknown facts centrally. Keep
   known cancellation effects without inventing IDs; no automatic resend of an
   unresolved write. Preserve fresh replacement IDs and racing-fill sizing.
   No-I/O capability/error branches use the same shared preflight completion
   seam. Remove every remaining legacy helper and its exact inventory entries.

Each implementation area gets its own failing-test commit, implementation
commit, exact ApeX-only inventory prune and caller appendix. Tests must fail by
the behavior assertion on the original source, then pass after adoption; compile
failure is not RED evidence. Recheck dependencies and fingerprints against actual
landing before starting each area. These phases confer no merge window.

### First slice acceptance details

At the base, every `FetchCandles` goes through `signed`/`SignedAuth`, but
`sign::request` deliberately emits no auth for GET `/v3/klines`. Keep that exact
unsigned request, sorting, supported intervals, daily-end omission, seconds/ms
conversion and malformed/all-invalid versus valid-empty handling.

Preserve the existing maximum three transport read retries (four physical
attempts without the signature-refusal resend, five with it), using shared
`ReadFailure` evidence under the original immutable
deadline. Preserve the native one-time signature-refusal read retry only after
shared read-failure admission; it cannot extend the deadline or mint a new
owner. Retry counters and response parsing are native facts; retry eligibility,
issuance, release and expiry remain engine policy. Preparation refusal must not
become a request-only transport fallback or a new network retry loop. Adopt the
landed typed `Venue::on_http_preparation_failed` callback for managed candle
preparation denial, preserving its `VenueError` separately from actual network
failure. Its default string-flattening request-failure adapter is insufficient
for this migrated path.

Tests pin: Prepare with public intent and no early HTTP; exact bound bytes at
claim; refused route and original-deadline failure; expiry while route is parked;
accepted result before deadline versus suppressed late response; duplicate and
reused command IDs; exhausted retries with one original deadline; mixed
transport/signature failures with separate retry budgets; callback after
disposal; and unchanged native successful/invalid-row/empty output. Use the real
prepared handshake in targeted controls rather than only an immediate driver.

`validate_read_http_delivery` applies before response publication. Do not gate
terminal timeout rejection on its live-deadline check: an expired current owner
must still finish through the engine's contextual failure path.

## Caller-visible obligations

This plan changes no caller behavior. Each implementation PR appends a CALLERS
section to `RUST_CUTOVER_CALLERS.md` describing only its landed behavior. For
the first candle slice, callers must provide managed transport preparation;
request-only custom transports fail explicitly. Route preparation and retries
consume the same original raw timeout, so an old late success can become typed
failure. Keep successful candle JSON and existing unsupported-interval/row errors.

Private-write follow-ups must document central Unknown at the immutable deadline,
known partial effects and exact reconciliation-only IDs, and preserve error
metadata through worker/hosted replies. The audited downstream targets are
`apps/web/src/order/sendOrder.ts`, `createMarketOrder.ts`, `createLimitOrder.ts`,
`createConditionalOrder.ts`, `createScaledOrder.ts`, `updateOrder.ts`,
`cancelOrders.ts` and worker/hosted error receivers. All-actual-failure history
exhaustion rejects as unavailable. Preserve the existing validated-success cache
selection; cached rows do not independently establish complete REST coverage.
Custom transport callers must adopt prepare/bind/claim with the SDK. Do not
claim NX integration, deployment, live credentials or new products from native
fixture evidence; no upstream checkout changes are part of this lane.

## Verification and independent review

Local code gates use `CARGO_BUILD_JOBS=2`: `cargo test -p
safe-cex-venue-apexomni -p safe-cex-venue-binance`, touched-crate fmt and strict
all-target Clippy. Run both `python3 scripts/check-request-context.test.py` and
`python3 scripts/check-request-context.py` from `sdk-rs`, plus bans. Ensure every
unowned manifest path/count remains identical. No local WASM release build,
replay sweep, load test or timing gate; CI owns those checks. Yarn uses the
existing NVM Node 22 Corepack executable.

Independent source/adversarial, caller and full-map reviews must answer: **is
this a per-venue fix of a cross-cutting concern?** Expected answer: **No**, only
native semantic declarations and adoption of the engine lifecycle. A missing
common mechanism stops that area and is requested from its core owner. Post
findings, fixes and proof in the PR, maintain a background CI watcher, and ask
the overseer for a merge window only after clean reviews and green exact-head
CI. Respect the separate revert guard and porcelain-only merge rules.

## Exact frozen inventory

The following source locations refer to the immutable base above. Every full
SHA256/count comes from `scripts/request-legacy-sites.json` and was checked
against the unchanged scanner. Totals: **6 source paths, 35 distinct
path/fingerprint entries, 45 conservative lexical occurrences**. Repeated
fingerprints in different files are distinct inventory entries. This inventory
includes native codec macros; it is not a count of HTTP requests.

Independent plan reviews (source/adversarial, caller, full-map) confirmed this
scope. Corrections incorporated: five combined physical attempts with separate
retry budgets, explicit typed preparation-failure handling, atomic snapshot pair
before independent balance, successful-cache caller wording, and discovery/
candle helper mappings. All three architecture answers are No. The unchanged
scanner matches the exact map; its 13 adversarial controls and current-inventory
check pass. This is plan verification, not implementation acceptance.

### `venues/apexomni/src/account.rs`

| Fingerprint (SHA256) | Count / base lines | Native site | Destination |
| --- | --- | --- | --- |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 / 901 | `cx.done(command, data)` | IDENTITY |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 2 / 902, 1239 | `cx.fail(command, error)` | IDENTITY |
| `1ab359385d803e45a7df42af0bdf53a41ba77bac050ce89c155c3462e61aba4c` | 1 / 906 | `cx.fail(command, "ApeX account identity payload missing or invalid")` | IDENTITY |
| `569abc443c07aca5127ee71c347600be68bb35c14aeae80c3a14b2f5f2642980` | 1 / 1060 | `cx.fail(command, error.to_owned())` | IDENTITY |
| `be0045610ff1c1d9208aa4b3677369be1e253a1fd78bb1eecd49f3636a057f85` | 1 / 1230 | `cx.fail(command, "ApeX account identity requires API credentials")` | IDENTITY |

### `venues/apexomni/src/history.rs`

| Fingerprint (SHA256) | Count / base lines | Native site | Destination |
| --- | --- | --- | --- |
| `5852ac5090c95388040b5d9d0ca6e3a85232e3274fedb5d8e7af7687e22c6184` | 1 / 345 | `cx.fail( command, "ApeX Omni cannot provide definitive client order ID lookup", )` | LOCAL |
| `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 3 / 349, 364, 368 | `cx.done(command, "[]".into())` | LOCAL |
| `119391c79a96f547ad746a782d0cfea34bcf6b6aa4e7bcf6a358b964cae4664a` | 1 / 351 | `cx.fail(command, "ApeX Omni does not support public trade history")` | LOCAL |
| `476319e239cb94cfebf5383b6c2f8bc9e3ee2bf66f70ed4ce32d3dcc1216dcb8` | 1 / 399 | `vec![ ( "symbol".into(), request.candidates[request.candidate].clone(), ), ("limit".into(), request.limit.to_str...` | LEXICAL + HISTORY |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 / 427 | `cx.fail(command, error)` | HISTORY |

### `venues/apexomni/src/lib.rs`

| Fingerprint (SHA256) | Count / base lines | Native site | Destination |
| --- | --- | --- | --- |
| `97ed964385cc7a52fcd23993359dd831f672724592d73be7beda8c73e36fe2b9` | 1 / 122 | `sign::request(&self.options, cx.now, &request, legacy)` | SIGNER |
| `49ba30affa9b727ea990dacbdef5a8033af5fbd08fe892e2fe8fcc8e080c596a` | 1 / 137 | `cx.request_streamed_body(method, url, signed.headers, signed.body, at, pending)` | BACKGROUND / HISTORY; retain until shared streamed consumers migrate |
| `0adb175953636afd100d2ddc68ad421af677dc89c16a52471e5cb4c54aefe898` | 1 / 139 | `cx.request(method, url, signed.headers, signed.body, pending)` | CANDLE / IDENTITY / BACKGROUND / WRITE; CANDLE departs in slice 1, retain until all remaining consumers migrate |

### `venues/apexomni/src/public.rs`

| Fingerprint (SHA256) | Count / base lines | Native site | Destination |
| --- | --- | --- | --- |
| `e52117c4fdcdc2b7c6d4e4003f13bf5aeb4add7faea2dee1b8750b8bd5af9681` | 1 / 366 | `cx.fail(command, "ApeX candle request failed")` | CANDLE |
| `e0c19c15f81cbe99e7eeb92d801ff01c757bb964b6cf3c0520696a71ebfe17aa` | 1 / 376 | `cx.fail(command, "Malformed ApeX candle response")` | CANDLE |
| `2e623cc117fc51b66d1fb7d2f4eb3646f70f72f0db14a1fd6b1ad0050316c6e4` | 1 / 380 | `cx.fail(command, "Invalid ApeX candle timestamp")` | CANDLE |
| `cc3aaf139ca7524fcb9612eaa4a98f366cfa321c229005f143d39d29f200a5ef` | 1 / 407 | `cx.fail(command, "No usable ApeX candles")` | CANDLE |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 / 415 | `cx.done(command, data)` | CANDLE |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 2 / 416, 881 | `cx.fail(command, error)` | CANDLE |
| `569abc443c07aca5127ee71c347600be68bb35c14aeae80c3a14b2f5f2642980` | 1 / 437 | `cx.fail(command, error.to_owned())` | CANDLE |
| `ea54413cb720d7c3e45aaf3bb9d7b167d66e7bfae2e044e17678ef461eb01056` | 1 / 853 | `cx.fail(command, i.unsupported("ApeX"))` | LOCAL |

### `venues/apexomni/src/sign.rs`

| Fingerprint (SHA256) | Count / base lines | Native site | Destination |
| --- | --- | --- | --- |
| `d5ae9df020daef6e1e8d4ddd22379cdfb8c715c03a570e7c70aa45576e7735cd` | 1 / 108 | `format!("{}?{serialized_query}", request.path)` | LEXICAL + SIGNER |
| `f71a3475f7a23850df5c38c255639f13d228eeebd3c67f93ca006548c48ea847` | 1 / 130 | `matches!(request.method, Method::Get \| Method::Delete)` | LEXICAL + SIGNER |
| `895aa96402247f510e57c4338213c3addc3ca6dfe7ee86a7aa5c2cb3c76ee829` | 1 / 152 | `format!( "{timestamp}{}/api{}{}{}", request.method.as_str(), request.path, if query_method && !canonical.is_empt...` | LEXICAL + SIGNER |

### `venues/apexomni/src/trading.rs`

| Fingerprint (SHA256) | Count / base lines | Native site | Destination |
| --- | --- | --- | --- |
| `ae3209100adb7617d3a60abc05d13961f6b17901cc02551743ff77b75e18b37f` | 3 / 534, 562, 683 | `cx.fail(command, not_sent(error))` | LOCAL |
| `1b543faa4b7a8b42bb71fa346816c4d1714b567a9447ba6e389eb6a6bb09d7d0` | 1 / 546 | `cx.fail( command, not_sent("ApeX Omni amendment requires a price or amount"), )` | LOCAL |
| `2b23d2f28c9a3c53273bdc5a8a0d1d739838cab4d96623c7b7a1a9e76248f09c` | 2 / 580, 606 | `cx.fail( command, not_sent(format!("ApeX Omni market {symbol} not found")), )` | LOCAL |
| `358900b9d751eae925c51af3d73875b67e9215403f2ebefed9f5e3f947147cf3` | 1 / 613 | `cx.fail( command, not_sent("ApeX Omni leverage must be finite and positive"), )` | LOCAL |
| `bcd45e2e77893c71604dfc34d2a327ce254b90adc4e9178be8643e022a6f4219` | 2 / 643, 658 | `cx.fail(command, error.kind(ErrorKind::Unsupported))` | LOCAL |
| `0b656f6ecb232b6e58dda360591ebef081f74c048b07c021b2589b269fb21488` | 2 / 645, 655 | `cx.done(command, "null".into())` | LOCAL |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 / 669 | `cx.done(command, data)` | LOCAL |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 / 670 | `cx.fail(command, error)` | LOCAL |
| `ca37246cf8052b21b57e7f1400b1026f755a6bdaadfe661eb481c929ddcc2d68` | 1 / 714 | `cx.done(job.command, result)` | WRITE |
| `6456c3d2f1245d934cb2d038038ae81a82d5e9fd85cf29070c87d948a57b8ea0` | 2 / 750, 780 | `cx.fail(job.command, not_sent(error))` | WRITE |
| `06eea309fa05c237f809ccb4c45df7a36798cfb0bcb6ace2f22b4b3d7d6f0ef3` | 1 / 822 | `cx.fail(job.command, error)` | WRITE |

## Owner assignment update

After the inventory checkpoint, the overseer assigned ApeX signer adoption to
`codex-sy8pn5` (ApeX first in the non-HMAC signer lane). Background-session
Context belongs to `codex-spc1lj`. Send signer requirements before private-read
work and coordinate overlapping `lib.rs` changes before either source PR opens.
The first unsigned candle slice leaves `signed`, `send_signed`, `sign.rs`,
account/history/trading reducers and session authentication untouched.


## Current first-slice checkpoint

The original source compiled with seven new candle controls, then all seven
failed at the no-send-before-preparation assertion. Regression commit:
`84c8fedba5d`. The implementation is a narrow Pending/callback adoption plus one
extraction of the existing native API-envelope verdict, preserving the legacy
signed branch and the native read retry budgets. Existing daily-bound/sorting
and malformed/row fixtures use the shared prepared-action driver without changing
assertions or clocks.

The scanner delta is only `venues/apexomni/src/public.rs`: 9 to 2 occurrences,
six removed fingerprint entries and one count decrement. ApeX becomes 29 entries
and 38 occurrences; global 1,226 to 1,219 at this base. Every unowned map remains
identical. The remaining two public entries are interval refusal and initial
context-issuance refusal; private/signing/local migrations still must reach zero.

Actual shared typed conversion is a prerequisite: `claude-dazzy3` owns PR #1808
for `request_context::Error::{kind, venue_error}`. Compilation/acceptance waits
for its actual landing; no venue-owned classification shim or engine edits are
part of this branch. Host disposal is exercised by the existing shared managed
HTTP host controls in CI; native venue tests exercise retirement and late/reused
callbacks. No claim of new ApeX-specific host disposal proof is made here.


## Authorized reviewed dependency stack and passing native verification

The overseer authorized stacking exactly reviewed #1808 head
`9b639939e378ee3d582c06b24fd068c095eded0b` before its actual merge so this
slice can compile and get CI. Porcelain merge `9e5bab721ea` is conflict-free;
the two inherited engine files exactly match that reviewed dependency. This
supersedes the earlier actual-landing compilation hold only for this authorized
stack. Re-integrate actual `origin/feat/rust-rewrite` after #1808 merges before
final readiness/revert guard and a separately granted merge window.

All eight new candle controls pass, including the seven original-source
assertion-RED controls and the native-error compatibility matrix. The complete
ApeX and Binance native suites pass, strict all-target Clippy for both crates
passes, scoped formatting and bans pass, and both request-context checks pass
(13 adversarial controls plus exact shrinking inventory). Source/adversarial,
caller and full-map reviews of the integrated boundary are clean; all answer
the architecture gate No. These are local native and static proofs, not rebuilt
WASM/replay/size/timing or deployment evidence; CI remains required.

Final scope against the initial base: nine lane-owned docs/manifest/venue/test
paths and the two inherited shared API paths. Every other inventory path is
unchanged; only the seven ApeX public completion sites are pruned. Public tests
retain all original assertions and event clocks; only their two request-drain
calls adopt the shared prepared executor.

Authoritative replacement lane IDs are `claude-19pt8s` for ApeX signing and
`claude-55ao7t` for background-session Context. The signer lane has reserved this
slice's `lib.rs`/public candle regions and will integrate its actual pin without
reshaping the extracted native verdict helper. No private signer/session source
is migrated by this slice.


## Actual error-conversion integration

Porcelain merge `781010fff81` integrates actual #1808 landing `73d04380dd9f21d75bdd023353e10a1ba1c5e801`, including actual #1785 and Bitget #1801. The only conflict was appended CALLERS content: all incoming sections precede the unchanged ApeX appendix, retained once verbatim. The integration guard lists exactly the nine lane-owned paths; no engine/Binance/source reversion or unexpected deletion remains. The exact ApeX-only seven-occurrence manifest prune passes the unchanged checker and all 13 controls. Scoped formatting, strict Clippy and bans pass; the full ApeX/Binance suite is rerunning on this actual tree before readiness. Fresh CI and a separately granted overseer merge window remain required.

## Actual typed HTTP failure integration

Porcelain merge `8fd785b4ea5` integrates actual #1816 (`3ac2592c361`)
and #1838 (`bd52632f2c8`). The candle callback retains the engine's typed
failure through its existing read retry budget. An undecodable response now
exhausts as `Internal`, without a write outcome, instead of being reclassified
as `Network`. Regression `03c9ac76048` compiled and failed on that exact kind
assertion before the callback change. Its final no-retry assertion allows the
engine's normal cancellation action and rejects a new Prepare.

The unnecessary generic capability-probe fixture carry is reverted by
`5e400ed4c20`: that probe exercises FetchOrderBook, so this candle slice does
not need it. Final intended scope remains nine own paths with no engine or
registry test delta. Command-entry, signer and session adoption remain separate
slices. Focused/full native checks and independent incremental reviews are
recorded on the PR before readiness; fresh CI and an overseer window still apply.


## Phase two: genuine candle command entry

Base: actual `bc91dc15d705f7d388ab3df46fa8c3ffb9395822`, after the first managed candle slice #1804.
Use actual #1830 `admit_http_command` in `public.rs::start_candles` with the
existing public Read policy, 20-second root and read retry budget. The engine
owns initial rejection and its terminal; remove the helper's numeric command ID,
error return and the caller's numeric failure fallback. A revoked entry has no
completion rights and returns without a terminal. Keep native interval
validation before entry, and preserve query bytes, retries and delivery codecs.
No signer, session, engine or private source edits are needed.

Regression controls first: invalid command clocks and initial context capacity
must each reject once with the shared typed Read error and no write outcome,
without HTTP/preparation/timer work. A refused entry must not occupy a generation;
a fresh query and an existing parked owner must still complete normally. Verify
these controls assert RED on the original helper before its replacement.

Prune only the removed initial-failure fingerprint from ApeX public.rs, leaving
its unsupported-interval preflight for the reviewed local-completion path.
Append the resulting initial-error contract to CALLERS. Run all existing candle
controls, full ApeX/Binance tests, strict all-target Clippy, scoped formatting,
bans and both request-context checks. Independent source/adversarial, caller and
full-map reviews must answer the architecture gate. No merge during the current
MEXC/header-seam hold; genuinely integrate any newly landed shared engine seam
and obtain fresh CI before a later venue window.


## P0 write adoption, classifier pin 2026-10-23

Audited actual source: `6741c1c6609437e32090c76745a56ca6c2262445`.
This appendix supersedes the earlier draft WRITE API names. It is a plan and
original-source regression specification; it does not migrate production code,
close an audit row, prune inventory, or claim a managed signer is landed.

### Reverified failures and native evidence

| Classification audit row | Current source / trigger | Required caller result |
| --- | --- | --- |
| #5 | `src/lib.rs::api_response` recognizes code 20016/401 or any nonzero code whose message contains `sign`, before checking 408/5xx; `response` resends with legacy signing. | A released write with 408/5xx is Unknown and never earns a resend, including a signature-looking native body. |
| #35 | `src/trading.rs::failure` leaves an unlisted numeric code with no outcome. | An unlisted code is explicitly Unknown, retains native code/message, and never proves refusal. |

Existing numeric distinctions are 401 (AuthInvalid), 429 (RateLimited),
20016 (signature refusal candidate), and success 0. There is no complete ApeX
refusal table in current source. Move only independently justified existing
codes into one `classify::CodeTable`: `success: &[Key::Int(0)]`, explicit
`unlisted: Verdict::Unknown(None)`, and no bare-status refusal rows without a
cited protocol guarantee. The old `contains("sign")` predicate is not native
refusal evidence; do not turn it into a message row. Freeze the exact 20016
refusal/resend mapping with the signer owner before implementation. Preserve
native zkLink and primary/legacy HMAC bytes, never broaden retry eligibility.

### Managed conversion and prerequisites

1. After the signer/provider prerequisites (#1839/#1841), declare a genuine
   command-owned Mutation with the real credential and immutable raw bound.
   `admit_http_command(Policy)` is actual (#1830); queued preflight/multi-leg
   commands still require the shared Plan/CallWindow admission design. No dummy
   Read/Background context may authorize a write. Keep atomic native preflight,
   entropy/ID allocation, virtual brackets and cancellation confirmation.
2. Carry the exact Context through Pending and the managed signed request.
   A native complete ID-bearing acknowledgement reports
   `http_write_fact(ctx, WriteFact::Accepted { order_ids }, None)`. Decode the
   integer code strictly, not through f64. A nominal success without a valid ID
   reports `http_write_unproven`, not Accepted or an automatic retry.
3. In the exact delivered callback, call `classify_write(ctx, &TABLE, native,
   message)` and then `http_write_fact` with `WriteFact::Failed`, or use
   `http_write_failure` for a whole failure. The engine supplies status, intent,
   attempt provenance and 408/5xx precedence. Keep native code/message intact.
   Only its opaque `ResendGrant` can feed `http_resend_context`; legacy signing
   selection is a recipe input on that same logical leg and original deadline.
4. Keep accepted sibling IDs as journal facts; use caller-ordered
   `http_write_batch` only for a real single native batch acknowledgement.
   Retain native stop/replacement sequencing and racing-fill sizing. Remove
   `Job.effective` outcome demotion only with the full command journal adoption.
   Narrow replacement Place intent with `http_write_leg_intent` when required.
5. Complete once through `done_context`/`fail_context`; engine CommandWrites
   aggregates prior releases, uncertain effects and reconciliation IDs. Never
   call engine-test-only `http_write_result`, transitional `classify::write`,
   local status ladders, text retries or a numeric completion fallback.

Pre-release `request_context::Error::venue_error()` / preparation NotSent is
valid for that attempt; a previously released sibling still determines the
whole-command outcome. Released transport/integrity failures remain engine
facts. Background session establishment belongs to claude-55ao7t; signer
recipes and provider refs belong to claude-19pt8s. No production adoption in this
slice until required shared seams actually land.

### Original RED controls and acceptance

Add isolated ignored contract tests to the unoccupied `tests/trading.rs` using
its existing ready/order/request fixtures, without new test credentials. Run
explicitly with `cargo test -p safe-cex-venue-apexomni --test trading
p0_write_ -- --ignored --nocapture`. Each must compile and reach its behavioral
assertion against the pinned original source: (a) a 500 + code 20016 signature
body emits no resend; (b) an unlisted code returns Outcome::Unknown with its
native code. The ignore reason declares the blocked adoption; remove the
marker only in the eventual implementation, keeping assertions verbatim.

Future GREEN controls also cover 408/503 signature bodies; documented 20016
refusal permits at most one grant; malformed/empty/contradictory success cannot
accept; retry grant from a sibling, old attempt or expired root is refused;
earlier accepted IDs survive later refusal/timeout; duplicate/late callbacks
produce one terminal; preparation failure after an earlier write does not
claim the whole command NotSent. Recheck exact legacy fingerprints from the
frozen inventory above at implementation time and prune only disappeared ApeX
entries. This plan/tests slice prunes none and adds no CALLERS landed claim.

Independent architecture gate: adoption must remain native codec facts plus
engine policy. Any missing reusable admission, signing or retry mechanism is a
shared prerequisite, never a second venue implementation. Implementation
validation remains venue + Binance tests, scoped strict Clippy/fmt, bans, both
request-context ratchets and exact-head CI; no local replay/timing sweep.

Local evidence at the plan/test head: both original RED tests compile and fail
by the intended assertion (extra HTTP resend; None instead of Unknown). The
normal ApeX + Binance suites pass with these two controls explicitly ignored;
scoped strict all-target Clippy, fmt and both request-context checks pass. This
is regression preparation, not GREEN adoption evidence. The landed classifier
source is #1837 (`e0322577e354739dde6d7e5011e74ab660145885`).


## Actual write adoption, phase 1 (2026-10-23)

Start exactly on reviewed #1874 head2891949d1e2. The overseer's subsequent
scope ruling permits two steps: phase1 adopts managed Context/CodeTable/journal
and proves positive remote HMAC cancel/settings; local zkLink placement keeps
working through the shared provider, while remote/keyless placement refuses
Unsupported/NotSent before foreground I/O until P8. The two placement REDs
remain explicitly ignored with P8 named, then become active in phase2 with a
positive remote zkLink order proof. No eager signing workaround is permitted.

Owned files: lib.rs (Options identity, managed Pending/typed dispatch),
trading.rs (native immutable plan, payload preparation, classifier facts and
contextual settlement), owned test fixtures and new managed-write controls,
this appendix, CALLERS and exact vanished ApeX manifest entries. Signer-owned
sign.rs and its recipe test stay unchanged; #1839's Cargo change is consumed
from its reviewed pin. The minimal public Options field is
`identity: Option<safe_cex_engine::signing::Identity>` for the signer owner's
registry admission. Opaque refs never enter the legacy eager signing path.

Reviewed prerequisites: #1839 at3eaa22397d0 (extra zklinkSeed/base64Text/provider)
and #1841 atcf70b07fac1 (Http::new/with_contracts, contract_payload and exactly
once placeholders). Plan source0f06f289fe4 is independently reviewed but its
feature-isolation pin is pending; use that owner's reviewed replacement before
final proof. Missing registry/SDK/host admission and positive remote HMAC
adapters stay with claude-19pt8s; P8 adds remote zkLink later. None is claimed
actual merely because its branch is inspectable.

1. Commit the Options identity handoff independently, then consume exact
   reviewed signer/Plan source without editing shared engine/provider regions.
2. Add original failing managed-HMAC cancel/settings controls before conversion:
   no early HTTP/signing before trusted Prepare, exact bound release bytes,
   real asynchronous remote-provider success, typed preparation refusal,
   expired/stale/duplicate command and no resend after an uncertain reply.
   Preserve native successful output, side effects and request/body/header bytes.
3. Begin genuine WriteAfterReads preflight before native validation terminals,
   retain the native FIFO/redo behavior, and admit a Plan only from the final
   immutable step set. Compute raw bounds from checked native attempts and
   pacing; no arbitrary120s root. Empty native work uses done_empty_plan before
   any admitted Plan, not a dummy accepted Context. Multi-call/split/amend
   execution must use the shared Plan API, never an invented Context loop.
4. Replace eager contract signatures with pure 36-byte native payloads and
   recipe placeholders. Release uses shared Http::with_contracts locally and
   Http::new for HMAC-only writes. Credential/body/query/legacy flags remain
   immutable recipe facts; exact delivery owns CodeTable/WriteFact settlement.
   Keep unlisted Unknown, 408/5xx precedence and opaque resend grants central.
5. Validate local zkLink regressions and actual positive remote HMAC behavior;
   retain the two P8-dependent placement controls as tracked ignored debt.
   Prune only fingerprints/counts that actually disappear; never rehash or
   move a legacy call merely to shrink the ledger. Record exact before/after
   totals and caller-visible limitations in the same PR.
6. Run touched venue + Binance, strict scoped Clippy/fmt, bans and both ratchets;
   obtain independent source/adversarial, caller and full-map reviews including
   the architecture gate. Publish a separate ApeX PR, wait for exact-head CI,
   and request a merge window. Then repeat Orderly, Backpack, Coinbase in order.

Architecture gate: native facts and adoption only. Any missing signer-mode
capability, admission, preflight, session or remote-wire support is requested
from its shared owner; no venue policy replacement or stale-context bypass.


### Plan-audit corrections and explicit shared dependencies

The actual implementation uses Plan custody and settlement throughout:
`validate_plan_delivery` before native reducers, classifier facts before
`finish_plan_call`, `plan_resend_context` for a granted resend, and
`done_plan`/`fail_plan` for the command. Context-only completion and resend
APIs do not replace Plan completion. Pending state retains the exact Plan,
Slot and Context; cleanup is fenced by owner and generation.

Before releasing any managed mutation, implement the bounded native
`capture_http_reconciliation` codec and restricted late-reply reducer. The
default hook rejects dispatch. Capture only native correlation facts, never
requests, recipes, credentials or signing material.

Independent review identified three shared prerequisites; none is replaced
with a venue workaround:

- **Conditional mutation terminal.** An amend cancel may succeed while racing
  fills eliminate its replacement. A declared conditional terminal must project
  the existing journal and retire the unreleased suffix under exact current
  delivery custody. `done_read_plan_after_data`, silent slot skips and
  post-admission `done_empty_plan` cannot authorize this. Codex-hcmq3f owns a
  separate engine follow-up after #1873; codex-uufub4 reviews API fit.
- **FIFO activation.** Native job B may wait behind A. A's delivery or an
  ordinary timer cannot admit B. Retained Preflight activation requires B's
  own engine-issued readiness custody. Codex-hcmq3f owns a separate engine
  follow-up, coordinated with the reconciliation-continuation owner.
- **Whole-command signing capability.** Single placement refusal at recipe
  preparation is sufficient for that attempt, but it cannot prevent an amend's
  HMAC cancel from executing before a later unsupported zkLink replacement.
  Claude-19pt8s owns a trusted identity-bound provider capability seam, queried
  before the first foreground release. Neither KeyRef text, extra-ref presence,
  raw Options secrets nor Signer::immediate proves provider capability.

The shared Plan owner confirmed the first two gaps are outside reviewed
#1873 and no composed implementation pin exists yet. Multi-request source
remains held until reviewed contracts exist. Four original HMAC controls
(cancel 503, setting 408, and unlisted-code cancel/setting) reproduce the
phase-one classifier failures independently of the two P8 placement controls.
The placement assertions remain unchanged and explicitly P8-gated.

The opaque Options identity handoff does not establish account/startup/session
readiness: those legacy paths still require their own managed conversion.
Remote proof must state its actual scope and use the real async host/provider
adapter, not relabel a local immediate signer as remote. No inventory entries
are removed by this preparation.


## Managed trading implementation checkpoint (2026-10-24, #1891)

The trading queue now begins genuine Preflight ownership, prepares native payloads
without signing, and admits the immutable serial Plan before release. Each write
uses `plan_leg_context_with_intent` and the shared signer recipe; the provider
checks all required algorithms before the first call of Place or Amend. Local
zkLink remains supported. A provider lacking zkLink refuses the whole command
before cancel/replacement I/O. A missing role-bound ref gives InvalidInput;
unsupported algorithms give Unsupported. Both are typed NotSent.

FIFO successors activate only through their retained Preflight readiness callback.
Amend racing fills use predeclared conditional terminals, with all earlier facts
projected by the common journal. The venue no longer demotes later rejection
outcomes or creates pacing timers. Ten-cancel pauses are declared Plan waits.
Retries remain the single native 20016 signature-refusal grant, with fresh native
20-second attempt windows; message substrings and bare status codes cannot grant
resend. Native integer strings use the same strict integer table as JSON integers.
Only native code zero on a 2xx response enters successful write reducers.

Bounded late metadata retains exact native cancellation targets and placement
client correlation, never request bytes, recipes or credentials. Late ACKs emit no Store update, command terminal, retry or auth state. The shared
Store revision fence alone cannot update ApeX's native account-read version fence;
an older account snapshot could otherwise resurrect a canceled row. Until that
composition is provided, late cancellation and placement evidence remain uncertain
and no optimistic row is published. Active successes still use the native fence.

All four original HMAC error controls are active. The three original provider
controls now pass, as do non-2xx/code-zero, quoted-code, partial cancellation IDs
and timeout/late-ACK revision controls. The native helper drives the existing
Prepare/Bind/Claim fixture at each test's actual event time. This proves local
provider behavior; the HmacOnly fixture is not a remote-signing proof. The native delegated-provider proof now drives real managed cancel and multipart
setting commands through Prepare, out-of-band Sign work, independently computed
apexomni-base64 HMAC answers, Bind and Claim. Literal Node vectors and remote-only
credential/MAC sentinels distinguish the answer from local signing. It asserts exact payload/MAC/header/body
bytes, no HTTP before the answer, duplicate receipt inertness and typed provider
refusal. It uses the signer owner's approved test-only venue construction while
registry admission is pending. SDK/browser wire proof remains separate.
The two placement REDs remain explicitly P8-gated (#1926), unchanged.

The exact ratchet prune removes only `venues/apexomni/src/trading.rs`: 11
fingerprints / 17 occurrences to zero. The shared legacy lib sender, startup,
account, history, public and session paths remain inventoried. Opaque identity
registration and SDK admission are separately owned by claude-m26ps4; no raw
fallback or registry migration is claimed by this checkpoint. Shared source
composition includes actual Plan core 632da7717e2, conditional terminal
cf6b2a89db8 and landing through 749a5677d0e. Retained-Preflight readiness #1896
and signer schema/recipe/capability #1839/#1841/#1899 remain reviewed stacks;
their actual landings must be integrated before final venue CI and a window.
The recipe owns the immutable REST and contract capability requirements, and
trading selects those declarations without a local algorithm policy.

Verification: the full ApeX and Binance suites passed 478 tests with only the
two directed P8 placement ignores. After the recipe declaration extraction,
the focused trading suite passed 41 tests with the same two ignores. Strict
ApeX Clippy/fmt, bans and both request-context checks passed locally.
Incoming Bitrue/Bitunix inventory drift was resolved by integrating their actual
landings through ae64150c89a, preserving their prunes. The signer owner independently approved the native delegated HMAC proof;
source/adversarial, caller and full-map reviews found no remaining source blocker.
