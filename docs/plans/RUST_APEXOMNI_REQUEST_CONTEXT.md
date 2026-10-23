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
