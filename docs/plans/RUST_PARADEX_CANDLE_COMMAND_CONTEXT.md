# Paradex command candle Read custody

Status: local plan and original controls; production, Cargo and maps unchanged.
Owner: codex-9czdjh. Native reviewer: codex-hcmq3f.
Base: actual `581bc2b77230c00ad6445dec23c78a9782370990`.

## Boundary and size

Manage only the unsigned command-owned `Command::FetchCandles` continuation in
`sdk-rs/venues/paradex/src/public.rs`. Preserve its native documented-kline to
TradingView fallback. The same `fetch_candles`, response and failure helpers also
serve subscription polls; those polls remain outside this slice. Their genuine
raw send sites stay inventoried. No renamed sender or raw compatibility escape
is introduced for a command.

The base request inventory contains 41 Paradex source occurrences: account 9,
history 9, identity 3, public 5 and trading 15. This command slice conservatively
removes TWO lexical terminals, public 5 -> 3 and family 41 -> 39:

| Base location | Exact fingerprint | Change |
| --- | --- | --- |
| public.rs:958, command failure | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 -> 0 |
| public.rs:1041, command success | `9eb2532ffa305903307527ed26561752ae2716a465c7347f6bb03d38987efb3d` | 1 -> 0 |

Retain the nonstreamed poll sender
`094a464645386463fbc12779495700591cd0fc4570d7257183d9928fc29b60f5`,
the streamed poll sender
`f4ff4cf72ac7033d5f05a949c3bebdf69840234f87e83ff025e63bdb7d7999b6`,
and discovery's conservative `matches!(request, Request::Markets { .. })` site
`d5350d6b7eb9bb6083de59c2c42f6ec4de997ccda8d28c277c0803969d9f2a77`.
All are count one. The plain lexical scanner reproduces this five-entry map.
No sender prune is claimed merely because command traffic becomes managed.

One logical command makes at most TWO sequential physical calls: documented
klines followed once by TradingView after failure. A wide supported interval
starts at TradingView and makes one call. Neither path pages or retries the
TradingView request. Native `limit` is ignored and must remain ignored.

## Ownership and existing work

Open #2039, `feat/rust-paradex-read-plan-history`, was inspected at exact
`193ce2612a50514808ca77e7028dce3804a3459a`. Its Paradex delta is a private-history
plan and `tests/request_context_private_history.rs`, with shared prerequisite
engine files and plan-index changes. It does not edit Paradex production source,
public.rs or command candles. This checkpoint adds only this standalone plan,
so it does not collide with that index or target. Recheck the actual file map
before originals or source; do not import #2039's unlanded engine ancestry.

Future owned paths are public.rs, narrow command-candle callback forwarding in
lib.rs, a new `tests/request_context_candle_command.rs`, this plan, and mechanical
own manifest/CALLERS documentation after source. Existing `tests/public.rs`
may need only genuine managed transport collection for command candle fixtures;
every native assertion stays. Do not edit discovery, poll subscriptions/timers,
public sockets/books/trades, account/config, private history, JWT, signing,
credentials, queues, engine, hosts or checker policy.

WEEX's `RUST_WEEX_CANDLE_CONTEXT.md` and TxFlow's
`RUST_TXFLOW_CANDLE_CONTEXT.md` provide the landed caller-root/managed-read
pattern. Paradex keeps its own two-route native geometry; it gains no pause,
history pager, Plan, Session or nonce dependency from those consumers.

## Native wire and result contract

Preserve the existing unsupported-interval then missing-market precedence and
their already-managed initial local refusals. Do not move those two branches
into a new finalizer or change their exact messages and metadata. Only the valid
remote command branch acquires the new retained caller root.

Both networks use the current `rest()` origin, GET, empty headers, no body and
default egress. No credential, signer, route override or private setup is needed.
Keep raw symbol insertion, query order, numeric formatting and interval mapping:

- S1/S5/S15/S30 remain unsupported. The documented set M1/M3/M5/M15/M30/H1
  first uses `/markets/klines?symbol={symbol}&resolution={resolution}` followed
  by `start_at`, `end_at`, `price_kind=last`. Supplied/default bounds are
  milliseconds; default end is that issuance's `cx.now`, default start is end
  minus 2,000 interval widths. Stream only `ArrayAt::Field("results")`.
- Other supported intervals start at `/tradingview/history?symbol={symbol}`
  then `resolution`. Supplied bounds append `from` and `to` as floored seconds;
  absent bounds stay absent, including on fallback. Finish the query with
  `countback=2000&price_kind=last`. D1/D2/D3/W1/Mo1 keep their native special
  resolution spellings. The TradingView call is not streamed.
- Kline rows retain the existing six-column decoder, accept trailing columns,
  keep native row order and millisecond timestamps, and poison the whole first
  response on an invalid row. Do not sort, deduplicate, clamp, filter to bounds,
  truncate to limit, or publish a partially staged prefix.
- TradingView `s=no_data` succeeds empty. `s=ok` requires all five OHLCV arrays
  to have the same length as `t`; multiply `t` seconds by 1,000. Keep every
  numeric/financial field, native order, malformed-array error and JSON shape.
- Keep `wire::check` status/envelope order and exact texts: `Paradex HTTP N`,
  `Malformed Paradex response`, `Paradex API error: ...`, the native list-stream
  error, `Malformed Paradex candles`, and transport text. Do not classify status
  as auth evidence or introduce a status retry ladder.

Native `public_failed` falls back from nonlegacy klines for status/envelope,
malformed row/list/stream and transport failures. Preserve that exception set;
do not silently tighten DATA or integrity failures in this custody slice. The
failed first call's rows are discarded before TradingView. A later TradingView
failure strictly fails the command once. No command result changes Store,
subscription generations, candle cache, socket interests or poll scheduling.

## Managed authority and failure order

In the genuine valid remote entry, use the landed original-receipt Read root
(`admit_caller_read_root(None, None)`) and the existing missing/expired entry
projections. Rejected already owns its one terminal; no numeric fallback or
replacement root may follow it. Respect shared invalid-clock, capacity,
duplicate-owner and deadline rules. Never reconstruct a receipt from CommandId.

Retain this root and the current physical Context in the command candle owner.
Represent command ownership with that retained authority, leaving the native
`CandleTarget::Poll { generation, revision }` behavior unchanged. For each actual
native call mint a fresh `http_leg_context` from the ORIGINAL root: Read,
credential None, timeout 20,000ms, no pacing, RetryPolicy::Never. Queue through
`request_managed`; only klines attaches the existing results stream. Polls keep
their current raw implementation. Never route a refused command through it.

Each streamed item and DATA envelope first validates exact root/current leg and
managed delivery through `validate_read_http_delivery`, before decoding, row
staging, envelope checks or final serialization. Hold the child until validated
synchronous DATA handling has completed. Before a successor, retire exactly the
previous child, discard all its rows, check the live original root, then issue
the sole TradingView child. At most one child is in flight. The second child's
bound is capped by the same original deadline; it does not inherit a renewed
caller wait or an additional retry budget.

For released failures, including physical timeout, bind the callback by the
engine's managed release dispatch and the exact retained child, then validate
the original root with `validate_read_root_context`. Do not require the timed-out
child to pass a DATA validator. Retire that child before native fallback or the
terminal. A failed/expired original root cannot start fallback or recover rows;
engine root expiry owns the one Timeout. Late or finished attempts are inert.

A typed preparation/queue/admission refusal is strict: retire any acquired
child and fail the original root with the supplied fields. It grants no native
response/failure evidence and cannot trigger a TradingView fallback. Keep local
pre-release NotSent when supplied. Released Network/Timeout/Internal causes
retain typed metadata instead of being stringified by today's lib.rs bridge.
For native DATA/status/codec string errors, preserve their native absent fields
through direct `fail_context`, rather than adding a new history classification.
Success completes the original root contextually. Settle and retire through
shared APIs exactly once; no `cx.done(command,...)`, `cx.fail(command,...)`, fake
history marker or replacement initial admission remains on the command path.

lib.rs typed preparation/HTTP/sign and stream callbacks may forward only this
new command owner; all noncommand variants delegate their prior behavior. A
command failure must never fall through `public_failed` as a legacy numeric
owner. Use existing root-retirement cleanup only if retained venue state needs
it; no new venue expiry timer, clock reconstruction or engine seam is proposed.

## Caller-visible policy for review

The raw candle packets contain no explicit `request_timeout`; do not call 20s a
retained native admission bound. The proposed NEW per-call admission-to-expiry
policy includes stuck Prepare/Bind/route waits. A long original wait allows up
to two successive 20s calls, not a 20s logical-command cap. A shorter original
receipt caps the entire command exactly. A live root may permit the native
fallback after the first physical timeout; root expiry never does.

New preparation/refusal and release fencing reject unclaimed/expired/retired
deliveries before financial parsing/publication. Native string errors retain
their fields; typed transport and preparation causes keep shared fields. Record
the final reviewed deadline/fallback/typed-projection contract in CALLERS at
production. Any reviewer requiring a stricter native fallback set must obtain
an explicit behavior ruling first; this plan does not approve that change.

## Proposed original controls, not executed

Write a separate new target only after plan review and permission to run local
checks. Use a real Engine, genuine caller receipts, Prepare/Bind/Claim and native
positive controls; compilation/setup failures are never RED evidence.

1. Both origins, documented/wide interval resolution, exact query/header/body/
   egress, absent and fractional bounds, ignored limits and native OHLCV output.
   Empty/no_data, extra row columns, unsorted/repeated rows and malformed parallel
   arrays pin native decoder behavior without widening it.
2. Kline status/envelope/invalid-row/missing-list/broken-stream/transport failures
   each issue exactly one TradingView fallback; no staged first-route prefix
   escapes. TradingView failure and a wide interval never issue a third call.
3. Valid command first yields Prepare rather than raw HTTP. Hold Prepare and
   Bind under a live 60s original wait through the new 20s child deadline:
   strict typed preparation Timeout, no fallback, one terminal, inert late reply.
4. A genuinely released first-child physical timeout under that live root
   follows the native fallback; a second-child timeout strictly finishes once.
   Root expiry during either child forbids fallback, output or a second terminal.
5. Real streamed chunks at/before original receipt equality, stale first-child
   DATA after fallback, duplicate DATA and command-generation reuse cannot stage
   rows, serialize results, touch Store or replace the current owner. Early DATA
   without final Claim cannot publish. Pin fractional root/child deadline caps.
6. Genuine 256 retained command-Read peers force initial Capacity before HTTP.
   Free one peer: root admission can fit while its first child correctly fails
   Capacity; preserve shared typed fields and revalidate all 255 peers. Free
   exactly one more peer: a fresh command succeeds, proving the failed root did
   not leak capacity; revalidate all 254 peers. No fake Context or excess frees.
7. Genuine NaN entry, duplicate live ID, zero wait, typed host refusal, queue
   refusal and same-ID recovery retain shared precedence/metadata and exactly
   one terminal. No provider calls, signing/auth counters or private I/O occur.
8. Existing public discovery, config/private startup delegation, reference-counted
   polls, 3s poll timer, poll generation/revision guards and socket recovery
   remain assertion-for-assertion preservation controls. Adapt only command
   fixture transport collection when later source actually requires it.

Disclose every suffix blocked by a first RED. Preserve original test bytes when
source begins; add independent held-permit, second-child and recovery assertions
rather than relabeling unexecuted suffixes as proof. Later source/authority review
must confirm fallback binding and narrow callback composition on the actual tip.
Run only approved scoped checks, serial CARGO_BUILD_JOBS=4; CI owns broad checks.
Exact source-derived manifest pruning and all foreign bytes/order require review.

## Current checkpoint and priority

The plan-first checkpoint created only this plan. No tests/builds, originals, production,
fixture, map, Cargo, shared index, push or CI dispatch have run or changed here.
Send the immutable local plan to the overseer and codex-hcmq3f for review.
The explicit plan-only instruction holds even if the plan is CLEAR.

Kraken recipe #2096 remains frozen at `ee270d49cb5`. It takes priority when
uufub4 supplies the corrected gated #2093 dependency: return to that branch,
plain-merge the exact corrected head without rebase, verify and take the one
authorized fresh PR run before progressing this new Paradex slice.

## Executed original checkpoint

Plan `5c381e9931e` received hcmq3f's bounded native/caller/API geometry CLEAR.
The overseer then authorized originals and production, scoped Paradex Cargo with
CARGO_BUILD_JOBS=4 and before/after Paradex WASM measurements. That authorization
supersedes the earlier plan-only hold; #2096 priority remains unchanged.

On unchanged production, the new standalone target executes **24 controls:
16 intended assertion REDs, 8 GREENs, zero ignored**. All compile and run; no
setup failure or missing API is counted. A preceding 21-control run (15 REDs,
6 GREENs) is superseded by the additive complete checkpoint.

Working positives pin both network URLs/full OHLCV output, documented/wide wire
time units, ignored limit/native ordering, all first-route status/list/row/API
fallbacks with prefix discard, transport fallback/no third call, no_data and
parallel-array failure, local validation precedence, absent bounds/defaults and
an already-finished first call's inert late failure.

Real REDs expose raw HTTP before Prepare (both initial routes and fallback),
missing caller/child deadline enforcement, unclaimed DATA publication, original
expiry during fallback and streamed-prefix completion, duplicate live command
issuance, NaN entry and full peer pressure bypassing shared admission. Held
Prepare/Bind, typed refusal, physical-timeout continuation and two-slot capacity
recovery suffixes are explicitly gated by their first assertion; their intended
later assertions are not claimed executed. All 256 pressure peers are genuine
command Reads, with exact one-at-a-time retirements and surviving-peer validation
in the recovery suffix. Source must execute those suffixes unchanged.

Only this plan and the new test target differ from the plan-first tree. Existing
fixtures, runtime, Cargo, inventories and shared files remain base-exact. Logs:
`/tmp/codex-paradex-candle-originals.log` and
`/tmp/codex-paradex-candle-original-clippy.log`. No push/CI or production claim.
