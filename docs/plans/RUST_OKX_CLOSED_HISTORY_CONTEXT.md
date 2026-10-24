# OKX reported closed-position Read custody

Plan first, 2026-10-24, codex-ih35js. Fresh branch
`feat/rust-okx-closed-history-context` from actual landing
`2371f5439a3d180e5ddef459a0c2d2bebfeb5604`. This checkpoint contains the plan
and, in a separate subsequent commit, original assertion REDs only. Production
and inventory stay unchanged until independent plan/control review. No merge
without an overseer train window.

## Scope and ownership

Migrate `venues/okx/src/closed.rs` and only its exact `Pending::Closed`
stream/response/failure/signing arms and retirement addition in `lib.rs`.
Reuse the actual caller-root, selected-read and history bridges; do not change
engine, signer, account/trading reducers, discovery, SDK mapping or checkers.
Keep the existing closed-position tests and their financial assertions intact.
The original controls live in new `tests/request_context_closed_positions.rs`.

claude-4yjlw0 owns account-cycle #1991 and discovery #1996. It explicitly cleared
these Closed regions: its source changes concern Account/Markets/Tickers, not
Closed. Preserve both early gates if their shared callback preludes conflict;
coordinate any source conflict with that owner. No other open PR touches
closed.rs or the new test at the initial file-list audit. This plan/test-only
checkpoint edits no shared callback or existing fixture.

## Exact current request inventory

The checker records **no direct fingerprint in closed.rs**: the legacy page send
is `self.list`, and legacy `Completion`/`CursorHistory` finalize in the engine.
Thus this slice has **0 direct manifest fingerprints/occurrences to prune**.
It retires a real legacy funnel consumer; it does not independently remove the
two shared lib.rs sites or claim that the OKX staging row is already removable.

The entire OKX map at the pinned landing is 7 fingerprints / 11 occurrences:

| Path | Fingerprint | Count | Disposition |
| --- | --- | ---: | --- |
| lib.rs | `2e9218906fd923fa6c83c648873ea2527c33dd1068d609c8c1ec343630b36911` | 1 | Shared sign_http funnel; retain until account, discovery and writes have migrated. |
| lib.rs | `b8eef2a79ada04918fd91d4c45f3a03a76d566ac79f733acf0c441b0fbbfc934` | 1 | Shared streamed-list sender; retain remaining consumers. |
| trading.rs | `2aa67be1113a12f9c2f147df5553229779430c188f2372b956154170cba2c409` | 1 | Other lane's write adoption; untouched. |
| trading.rs | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 3 | Other lane's write adoption; untouched. |
| trading.rs | `5d6691e305370ef1b5e22426e494d28b28936de80d5d5af079e9deae52eb33f2` | 1 | Other lane's write adoption; untouched. |
| trading.rs | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 2 | Other lane's write adoption; untouched. |
| trading.rs | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 2 | Other lane's write adoption; untouched. |

Current lib list consumers include Closed, four account call sites and
Markets/Tickers. Trading also calls sign_request directly. #1991/#1996 and
#1919 own the remaining account/discovery/write conversions. The checker owner
alone tightens STAGING_BOOTSTRAPS after a complete remaining-consumer audit.
No manifest, checker, exemption or fingerprint rehash belongs in this checkpoint.

## Native wire, rows and completeness

- Signed GET `{rest}/api/v5/account/positions-history?instType=SWAP`, optional
  encoded instId, then before=from_ms, after=to_ms/cursor, limit=100. Keep exact
  parameter order, network URL, demo header and the existing sign::Http recipe.
- Primary credentials gate the operation; keyless sessions return `[]` before
  market/time interpretation, even if a secondary slot exists. With Primary,
  unknown symbols also return `[]`. Do not invent time validation or filtering.
  Remote reads select Secondary else Primary once; scope/evidence stays None,
  because the native list funnel does not tag this as an account snapshot read.
- Keep raw response item bytes and order. Count every staged item. Only positive
  integer uTime participates in oldest; next cursor is oldest minus one. PAGE100,
  MAX_PAGES100, end_on_unchanged_cursor, nonadjacent-cycle handling and the native
  100 ms inter-page timer stay in the shared pager/native scheduling path.
- The output is an array of raw rows for the existing TS OKX mapper; no dedup,
  amount conversion, timestamp coercion, financial filtering or sorting is added.
- Native cap/cycle warning text, HISTORY_INCOMPLETE ownership and notice-before-
  terminal ordering stay intact. All rows from validated pages may survive.
- Live-root physical failure retains accepted-output recovery: no accepted rows
  means a typed strict failure; nonempty accepted output means only that prefix
  plus the existing owned INCOMPLETE marker. Failed-page staged rows never enter
  the output. Recovery depends on accepted output, not a page-number heuristic.
- Native closed_sign_failed uses the same recovery: first/no-accepted-output
  provider refusal is strict, while a refusal after accepted rows recovers that
  prefix. Pin this original baseline separately. The subsequent explicit
  overseer ruling changes provider/preparation refusals to strict through the
  exact root, consistent with #1950; preserve provider fields without retry or
  auth inference. This is an intentional change, not native parity.

## Managed geometry and caller changes

For native local empty results, directly admit the genuine caller Read root
before completing `[]`; a keyless local result issues no credentialed work.
For remote work use actual #1983 `SelectedRead::command_root` and its immutable
`ReadRoot`/`ReadLeg`, not a venue issuer wrapper. Fix Secondary-else-Primary
selection once, with the native primary-required gate. Bind the exact original
Context to `ReadCompletion::from_context`, then `ReadCursorHistory<String>`.
The opaque selected root and the history authority refer to the same root.

Each page leg is 20 s/Never, minted only at the existing eligible first-send or
100 ms dequeue boundary from that root. Its deadline is min(root, dequeue+20s).
Queue through ReadLeg with the existing recipe and streamed data envelope.
Retire the completed physical leg before deriving the next; never derive from
the previous child, renew the caller receipt, or add retry/Plan/session work.
Validate selected delivery and history admission before raw count, oldest,
staging or cursor/progress mutation. Timers alone grant no DATA/send authority.

Root = the engine's exact original caller receipt deadline: absent host wait was
already stamped120000 ms; supplied rustCommandTimeoutMs is immutable. Missing
receipt fails closed via shared admission, never a local default. This replaces
the old per-physical-request-only20s bound with a whole-call bound. Root expiry
is one strict typed Timeout with no rows, finalizer or INCOMPLETE marker. The
engine owns its timer and calls read_root_retired; remove only Closed delayed
entries matching that exact Owner, alongside the existing history cleanup.
Late data/timers/provider work cannot revive it or finish a reused command ID.

Local child-mint/submission refusal strictly rejects the still-live original
root without prefix recovery, typed through shared Read projection. Only a
proven revoked owner is inert. A released child timeout while the root remains
live uses native accepted-output recovery; engine root expiry stays strict.
Provider/preparation refusal strictly rejects the exact live root even after
accepted output, per the overseer ruling. Preserve typed kind/message/native
metadata; do not soften it to rows or emit an INCOMPLETE marker. Ordinary
transport/HTTP/live-child-timeout failures retain accepted-output recovery.

CALLERS must describe new invalid-clock/capacity local refusals, the immutable
whole-command bound and strict provider/preparation failure after accepted rows.
SDK fetchClosedPnl/fetchReportedClosedPnl catches/reports command failure and
returns[]; fetchHistoricalPositions/okxJournal uses report:false and propagates
rejection. Both consume fetchClosedPositionRows; recovered physical failures
remain marked through copyHistoryCompleteness. These existing catches and OKX
mapping stay unchanged. All OKX aliases share the same native operation; no
capability or SDK timeout configuration changes.

## Original controls and review gates

After this plan commit add real Engine<Okx> tests, running unchanged production:

1. Behavioral REDs for keyless/unknown-symbol admission at invalid clocks and
   finite missing receipt, plus full genuine registry capacity with live peers.
2. A first signed closed read must emit managed Prepare before signing/HTTP;
   delayed preparation is inside the immutable original root deadline.
3. Explicit short-root expiry during a page and during native100ms queue wait:
   equality fails strict, without partial rows/marker/new page.
4. Several successful15s pages cannot renew the default120s root. Exercise
   root expiry before response and queue timer, plus reversed equal-time order.
5. Dequeue capacity refusal preserves unrelated peer preparations and strictly
   drops accepted output. Expired command reuse plus old timer/DATA cannot
   publish into or retire the new owner.
6. Native working controls pin raw wire/rows, keyless precedence, Secondary
   selection, first failure versus accepted-prefix recovery and rejected staged
   rows. Preserve a working original provider-prefix baseline plus a separate
   desired strict-provider RED after accepted output. The implementation will
   update that one baseline expectation for the ruled policy, retaining this
   original checkpoint; no ignored contradictory tests. First provider refusal
   remains a working typed/no-output control. Existing
   cap/cycle/unknown-time controls remain untouched and run independently.
7. After adoption add exact child-deadline/preparation metadata and live child-
   timeout controls through the real managed adapter, keeping original RED
   checkpoints and existing financial assertions. No ignored/compile-error REDs.

Commit the plan first, then original executed assertion REDs separately, and
open a draft. Obtain independent source/adversarial, caller and full-map review
on the immutable plan/control pin before production. Implementation gets its
own commit, caller/audit/quality appendices and native/strict/provenance gates.
CI owns WASM/replay/size/CPU/memory; no local release build or budget change.

Architecture gate: No per-venue replacement of a cross-cutting concern. Native
closed-position encoding/cursor facts consume shared selected Read admission,
root expiry, child delivery, history rows/completeness and terminal authority.
No local root timer, issuer/finalizer wrapper, retry loop or classifier is added.
