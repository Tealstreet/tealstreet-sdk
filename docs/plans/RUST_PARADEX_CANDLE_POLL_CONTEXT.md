# Paradex subscription candle poll Read custody

Status: local plan only; originals and source await scope/native/API review.
Owner: codex-9czdjh. Proposed native reviewer: codex-hcmq3f.
Base: actual `9b05c9ac4528212b3c681f0f45f5980192b4342e` (#2102 landed).

## Boundary and exact size

Manage the unsigned subscription-poll arm of `fetch_candles` in
`sdk-rs/venues/paradex/src/public.rs`. Command FetchCandles already uses the
landed #2100 caller-root implementation and must retain that ownership. Polls
have no caller root or numeric command terminal: each physical route is a fresh
Background Read. Keep the documented klines to once-only TradingView fallback.

The base inventory has 39 Paradex production occurrences: account 9, history 9,
identity 3, public 3 and trading 15. This slice removes two real raw send sites:

| Base location | Fingerprint | Count |
| --- | --- | --- |
| public.rs:1165, nonstreamed TradingView poll | `094a464645386463fbc12779495700591cd0fc4570d7257183d9928fc29b60f5` | 1 -> 0 |
| public.rs:1173, streamed documented poll | `f4ff4cf72ac7033d5f05a949c3bebdf69840234f87e83ff025e63bdb7d7999b6` | 1 -> 0 |

Projected public 3 -> 1; family 39 -> 37. Retain the conservative discovery
macro fingerprint `d5350d6b7eb9bb6083de59c2c42f6ec4de997ccda8d28c277c0803969d9f2a77`.
Do not claim that entry disappears with the final two poll senders, rename it
to evade the scanner, change a checker, or rebaseline another venue.

## Ownership and exclusions

The only open Paradex-named PR found at planning is #2039,
`feat/rust-paradex-read-plan-history`, inspected at immutable
`193ce2612a50514808ca77e7028dce3804a3459a`. Its Paradex paths are the history
plan and `tests/request_context_private_history.rs`; its engine prerequisites
and shared index are outside this slice. It has no Paradex production/public.rs
delta. Do not import that unlanded ancestry. Recheck ownership before originals
or source. This checkpoint adds only this standalone plan, avoiding index edits.

Future owned source is public.rs plus narrow poll-aware forwarding in lib.rs.
Original controls belong in a new `tests/request_context_candle_poll.rs`.
Existing public fixtures may collect genuine Prepare/Bind/Claim for polls, with
every native assertion retained and every actual expectation change disclosed.
Later inventory/CALLERS/index/audit updates are only this slice's own entries.

Exclude command-root geometry, discovery, books, public socket supervision,
trade-driven candle updates, private history/account/JWT, signing, credentials,
selection, engine/host APIs, Cargo features, profiles and budgets. In particular,
preserve lib.rs's public-response `account.start` delegation. An unsigned poll
request does not mean that all native response delegation is credential-free.

Kraken #2096 remains held until #2093 actually lands, then recomposes by plain
merge under the overseer's order. This plan neither imports nonce machinery nor
starts a competing size build while uufub4's paired measurement hold is active.

## Native request and scheduling geometry

Polls start from three existing paths: first SubscribeCandles after markets are
loaded, completion of market discovery for subscriptions queued before loading,
and a matching native poll timer. A duplicate subscription increments the
reference count without issuing another immediate request. Final unsubscribe
clears the recorded poll timer and removes that native subscription generation.
Preserve the unknown-market/unsupported-second-interval notices and precedence.

Each cycle captures symbol, interval, subscription generation and candle revision.
Its bounds are `(now - 3 * interval_ms, now)` at that cycle's issuance. Both
networks keep the current REST origin, GET, empty headers, absent body and default
egress. Native query order and number formatting are unchanged:

- M1/M3/M5/M15/M30/H1 first use `/markets/klines`, with symbol, resolution,
  millisecond start_at/end_at and `price_kind=last`; stream `results` only.
- Other supported nonsecond intervals go directly to `/tradingview/history`.
  Keep the same resolution mapping, floored-second from/to, countback=2000 and
  price_kind=last. The first-route fallback keeps the original bounds/revision.
- Native status/envelope/row failure or released transport failure on documented
  klines falls back once. TradingView has no third call or automatic retry.
  Discard all first-route staged rows before fallback. Preserve no_data, parallel
  arrays, candle number decoding and native row order; do not sort/truncate.

Completion uses the last response row, not the maximum timestamp. Publication
requires the existing generation to match, then the captured revision to equal
the current revision, followed by the native timestamp/volume and changed-field
tests. Trade updates advance revision with wrapping_add and preserve their own
financial merge. Do not broaden that change to a new revision scheme.

Revision is a publication fence, not a physical-liveness fence. When a trade
advances revision during a poll, the old result cannot overwrite it, but the
current-generation cycle still completes cleanup and schedules its next poll.
Do not drop that completion or its native timer merely because revision differs.

The next poll timer is completion time + 3,000ms, including empty results and
the native final-failure cleanup. This is not a fixed 3-second wall-clock send
rate. Preserve the completion-time recovery condition exactly: last_trade older
than 20,000ms and last_recovery at least 30,000ms ago triggers the existing
trades-channel subscription refresh. The HTTP bound below is a separate policy;
it must not replace either trade-staleness threshold or public socket watchdog.

## Proposed shared custody

Each physical route calls `http_context(Scope::Background, Policy { Read,
credential: None, timeout_ms: 20_000, pacing: None, retry: Never })`, queues through
`request_managed`, and retains that exact Context in its Pending. The documented
route then declares `stream_response(..., ArrayAt::Field("results"))`.
No caller root, Plan, Session, provider batch, nonce, local issuer, owner map,
transport timer or raw poll fallback is introduced.

**NEW caller-visible policy:** 20s is admission-to-expiry, including Prepare,
Bind, route waits and transport. Native poll packets have no explicit timeout_ms
bound; this is not retained native transport timing. Each fallback gets its own
fresh 20s Context after retiring the previous one. There is no new logical 20s
cap over the two-route cycle, and no transport retry credit is granted.

Before streamed-item staging, status/envelope decoding or candle effects,
require `validate_background_read_http_delivery(&context)` and the native
subscription generation to remain present. Publish synchronously under that
grant; retire the Context only after the native publication decision. Complete
the remaining native timer/recovery bookkeeping once. Finish a physical Context
before issuing its successor, and retire on every final/error/queue-refusal path.

A released failure is bound by the engine's still-managed attempt and its exact
retained Pending. Check the subscription generation, retire the failed Context,
discard staged rows, then issue the once-only fresh fallback when eligible. A
physical timeout cannot pass the DATA validator; never demand one in that failure
callback. Preserve native DATA-error fallback after valid DATA admission.

An unreleased admission/queue/preparation refusal is strict for that cycle:
retire what exists, do not recursively try the second route under capacity
pressure, and use the current generation's native empty/failure completion and
3-second scheduling. Preserve the typed engine error internally; retain native
Warn/REQUEST_FAILED text for native failures. Any new refusal notice must be
pinned as a new contract, not presented as an unchanged native string. No command
failure, money outcome or authentication-health update is synthesized for a poll.

**New stale-work custody:** a retired subscription generation cannot decode,
publish, warn, start fallback, schedule a timer or trigger recovery. This extends
the existing generation publication guard to managed continuations. A current
generation with only a changed trade revision still gets its native cleanup.
Old Contexts are bounded and retire on stale callback, released failure or shared
expiry. This plan does not promise immediate HTTP cancellation on unsubscribe or
add new poll coalescing/single-flight semantics. If native startup can overlap
cycles, do not silently supersede one without a separate behavior ruling.

All nonpoll callback branches and #2100 command callbacks retain their existing
dispatch. Scope new guards narrowly; an early return in outer on_response must
not skip the existing account.start delegation. Preserve already-managed discovery
and command transport rather than wrapping it in another owner.

## Original controls before source

Use a genuine Engine with a thin forwarding Probe. Do not mint Context authority,
fake a DATA witness, or count compilation/setup failures as RED evidence. Preserve
raw-compatible native positives so the same assertions execute after adoption.
Record each first assertion and honestly identify suffixes blocked by that RED.

1. Both network origins, interval route sets, exact header/body/query/time units,
   last-row selection, financial values/order, no_data and malformed arrays.
   First-route status/codec/transport fallback once, staged-prefix discard,
   final-failure notice and no third request.
2. Initial and fallback Prepare before HTTP; real Bind/Claim before DATA; held
   Prepare and unclaimed Bind at live 20s equality; inert late provider/host
   completion with explicit no Bind, no HTTP and no candle publication.
3. Current-generation first physical timeout can start exactly one fresh fallback;
   fallback timeout finishes once with native next-poll scheduling. No DATA
   validator is required of the expired child. Typed prep refusal does not fall
   back, and exact cleanup/shared metadata is checked.
4. Genuine 256 retained Command Read peers cause Background admission refusal
   before poll HTTP. Retire exactly one peer: one Background physical Context fits,
   native success or fallback retires it, and all 255 survivors validate before
   and after completion. There is no extra command root/two-slot requirement.
5. Double subscribe/single unsubscribe continues; final unsubscribe and resubscribe
   fence old success, old failure, stale streamed rows and delayed host completion.
   No stale timer/recovery/notice/fallback. Current generation still functions.
6. A real JSON/binary trade during the pending poll advances revision. An old poll
   cannot overwrite the new candle, yet completion still schedules the 3-second
   timer and retains native stale-trade recovery eligibility. Keep the existing
   older-bucket/lower-volume controls and equality/greater-volume positives.
7. Completion-to-next-poll 3s timing, native 20s/30s recovery boundaries, queued
   pre-market subscriptions, duplicate reference counts, absent/removed markets,
   real nonfinite Event ingress and broken/nonUTF8 stream behavior.
8. Run unchanged #2100 command controls and existing discovery/public/native
   financial controls; prove command root completion and account-start delegation
   are preserved. No new socket lifecycle or private request authorization.

## Source, size and publication gates

Plan review precedes originals; originals must receive bounded native/API review
before production. The overseer requested this plan before source, so do not
infer source authorization from this document. No originals, source, maps,
profiles, shared index, push or CI are part of this checkpoint.

#2102's landed profiles are the new baseline. Paradex engine opt-level z and all
other profile/feature settings remain exact incoming; do not reuse the pre-#2102
1,339,096-byte artifact as the new size baseline. Before/after family Node/web
and combined measurements must use the same actual source/toolchain/path inputs,
fixed budgets and serial CARGO_BUILD_JOBS=4, coordinated with other size owners.
The approximately 75K headroom is planning context, not a measured new result.
No budget/profile adjustment or assumption that size headroom proves CPU fit.

Later run scoped Paradex controls/strict Clippy/fmt and full bans/ratchets/selection;
CI owns broad coverage. Preserve every unowned incoming path and foreign manifest
byte/order, prune exactly the two named poll senders after the scanner matches,
and disclose the new bound, strict pre-release cleanup and stale-generation
continuation behavior in CALLERS. Recompose the actual tip before the intended
freeze push, with docs incoming plus own suffix once. One PR-triggered Rust run;
no explicit duplicate dispatch or WIP push. No READY until qualifying CI/source
reviews and the auditor's composed-tree gates are satisfied.

## Reviewed scope and nonfinite-clock contract

The overseer approved this scope and its behavior changes after plan
`b34798899c2`: fresh unsigned BackgroundReads, native fallback/last-row/revision
publication and timers; revision change remains cleanup, with no new coalescing
or unsubscribe-cancellation promise. The new admission bound and stale-generation
suppression must appear in CALLERS. hcmq3f gave bounded native/caller/API direction
CLEAR on that exact plan, independently confirming the two singleton sites and
39 -> 37 family projection. Originals and exact source remain separate gates.

A real NaN or infinite Event clock must refuse Background admission without
HTTP, Prepare, Bind, a poll timer or trade-recovery scheduling. Do not call native
`finish_candles` blindly on an invalid-time admission error: `set_timer` adds its
delay to the invalid clock. At finite current-generation admission refusal,
retain exactly one native completion + 3s timer. This is a guard on the new shared
admission failure path; it does not change native finite-time cleanup.

## Original-control checkpoint

The local target executes 30 tests: **17 intended assertion REDs, 13 GREENs,
0 ignored** against unchanged production. Native positives cover both origins,
all declared nonsecond intervals, exact 3-interval bounds and time units,
last-row financial values, one fallback on status/envelope/row/transport/broken
stream failures, empty/malformed TradingView cleanup, reference counting and
stale success, pre-market subscriptions, live revision cleanup, and strict 20s /
inclusive 30s recovery thresholds. Revision-plus-recovery uses two physical
routes, each inside its own bound, with a trade after the original capture.

The REDs expose initial/fallback raw HTTP before Prepare, held Prepare/unclaimed
Bind, unclaimed DATA publication, missing first/second physical deadlines,
strict first/later preparation refusal, genuine 256-root capacity, real NaN and
Infinity entry, stale failure/broken-stream fallback, and staged publication at
20s equality. The streamed equality control observes two actual item callbacks;
the stale malformed control observes one before removing its generation.

Preparation/deadline cases fail first at raw-HTTP exclusion. Their later Bind,
Claim, typed metadata, timer, no-late-Bind and no-third-call suffixes are gated and
are not counted as executed evidence. Capacity fails at its initial no-HTTP
assertion: exactly one peer retirement plus validation of all 255 peers before
and after recovered success is a disclosed gated suffix. Native positives use a
raw-compatible collector which performs actual Prepare/Bind/Claim after source;
the unclaimed DATA control deliberately extracts the packet without that helper.

A preliminary fixture counter included eight startup items; it was corrected to
measure the per-request delta before this checkpoint. That fixture failure is
not a custody RED. No source, Cargo, existing fixture or inventory change is part
of these originals. The target is reviewed independently before source.
