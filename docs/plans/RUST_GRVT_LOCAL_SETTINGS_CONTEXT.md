# GRVT fixed local settings

Plan-first base: actual `7fe49f6b964efb47c96f996f1cacf38358a04a4d`.
The initial checkpoint adds this plan and a dedicated original-control target.
Production, existing tests, Cargo and inventories stay exact until independent
API/native original review. No push or CI for a plan/RED checkpoint.

## Scope and inventory

Three numeric terminals in `trading.rs::trade_command` are independent of
credentials, signing, selection, Sessions, Plans and queued writes:

| Native branch | Terminal occurrences | Existing behavior |
| --- | ---: | --- |
| SetMarginType, either requested mode | 1 | Unsupported/NotSent: GRVT margin mode cannot be changed through this integration |
| SetPositionMode, hedged=true | 1 | Unsupported/NotSent: GRVT supports one-way positions only |
| SetPositionMode, hedged=false | 1 | Store hedged=false, then [] success |

All three run before trade_plan and its credential/validation work. Preserve
that order even without primary credentials, in public-data-only mode, with an
unknown symbol, or while native writes are queued. Neither requested margin
mode performs a remote write. One-way changes only the existing Store flag.

Canonical checker inventory on this actual base is GRVT48 occurrences, of
which trading has8. Prune exactly3, giving trading5/family45 on this base:
405d7ebc1c72 decreases2->0; a24b9b2de471 decreases1->0. Only this manifest key
may change. The candle/book PRs separately prune lib/depth keys; do not claim
their pending removals or a fixed combined family count after another landing.

This slice touches trading's three fixed exits plus imports, the new test and
plan, its own manifest key and standard docs at publication. It does not touch
lib.rs, account/login, candles, depth_query, identity, history, signing, crypto,
registry, engine, hosts or workspace Cargo. Thus it is independent of the
currently frozen #2062/#2068 sources and the completed combo5b source window.
Confirm no new owner hold before production.

## Shared authority and native projection

One-way success uses done_local_setting in the genuine Setting callback. Keep
[] exact. The Store set_hedged(false) runs only in the prepared projection
closure, after every reserve. There is no Context, credential, HTTP leg or
journal for this synchronous setting success.

The two refusal lines first call admit_http_command with Mutation, no credential,
20 s and Never, then fail_context with the unchanged native VenueError. These
are immediate local failures, so this policy grants no deferred continuation
or physical work. A shared admission rejection owns its single terminal; return
without a numeric native fallback. Mutation NotSent and Unsupported survive the
empty-journal projection. No key or signer is selected.

Admission/capacity, invalid-clock and caller-receipt refusal therefore precede
the native fixed result and Store effect. Disclose this newly enforced ordering
in CALLERS. SetMarginType/SetPositionMode are Intent::Setting; the fixed failure
uses a Mutation Context, while only the actual success uses LocalSetting.

LocalSetting checks command-owner capacity, not Background occupancy alone.
Capacity controls must retain256 genuine Command Read owners and validate all
255 survivors after retiring the exact first peer and recovering the setting.
A duplicate command cannot replace its original owner. The existing shared
foreground-only Capacity projection is Internal/NotSent; no new retry cause,
background quota, timer or admission helper is introduced here.

## Original controls

Drive genuine Engine entry through a thin Venue delegator. Forward the actual
GRVT on_command for every native setting. The fixture may seed Store hedged=true
and admit/retain independent peer commands; it must not synthesize the tested
native terminal, create credentials or bypass command admission.

Separate controls for cross and isolated refusal, hedged refusal and one-way
success pin native message, kind, NotSent/[] and unchanged Store except one-way.
Run them without credentials and in public-data-only mode. Both margin cases
share one lexical site, but keep their native controls distinct.

For each native case, fill256 real Command Read owners first, then assert one
shared Capacity failure before native completion/refusal, no CommandError,
no HTTP/managed/provider/socket action and no Store projection. The recovery
suffix must retire the exact first peer, re-run the same command, assert its
native result, and validate all255 retained peers. On original source these
capacity suffixes are gated by the first failing assertion; never claim them
executed merely because the target compiled.

Add independent NaN-clock cases for the success and refusal through Event::Command
rather than a checked helper that rejects before the venue. Also pin zero caller
wait and duplicate-owner preservation with genuine roots. Original REDs must
compile and fail assertions on the unchanged production tree; missing methods,
fixture/setup failures and source implementation are not RED evidence.

## Exclusions and source gates

Keep initial trade_plan failures, queued trade_next revalidation and every
trade_finish terminal unchanged. A dequeued command is not a fresh command
callback and cannot be re-admitted locally. No signing/raw-credential cutover,
nonce, builder authorization, account/session, accepted IDs, financial reducer,
ACK, cancellation/replacement, reconciliation or native queue change belongs
here. No local typed classifier, partial aggregate or numeric completion bridge.

Plan precedes compiling originals; fe7lgm reviews API/custody and9czdjh native
behavior/preservation before source. Production follows only the approved scope
and CLEAR originals. Preserve the original test assertions at adoption. Verify
one scoped changed-crate target at a time with CARGO_BUILD_JOBS=4, strict scoped
Clippy/fmt and all ratchets. Check the own target size before builds and clean
between builds if over15G; no ENOSPC-window result is release evidence. CI owns
workspace, feature, WASM and performance checks. Push only an intended freeze
head as a draft, one PR Rust/head, then source reviews plus green CI permit READY.

## Original execution checkpoint

The unchanged production tree compiles and runs all15 dedicated controls:
6 intended assertion REDs,9 working GREENs,0 ignored. The four full-capacity
cases fail first at native success instead of refusal (one-way) or Unsupported
instead of shared Internal (cross/isolated/hedged). The two genuine NaN Event
cases fail at [] success or Unsupported instead of InvalidInput. These are
runtime assertions, with no compile, fixture or missing-method failure.

Working controls pin all four native variants without credentials, all four
again in public-data-only mode, two zero-wait refusals before venue entry and
two duplicate controls retaining the exact original Read owner. Each capacity
recovery suffix validates all255 survivors, but remains unexecuted on original
source after the earlier expected RED. Production, existing tests, Cargo and
all inventories remain byte-exact7fe at this checkpoint.

### Margin invalid-clock strengthening

Before production, append an independent margin NaN Event control as requested
by the API reviewer. The unchanged target now executes16 controls:7 intended
assertion REDs/9 working GREENs/0 ignored. The new case reaches the native margin
Unsupported refusal instead of shared InvalidInput, independently pinning this
branch. All previous15 test bytes remain an unchanged prefix. No production,
existing fixture, Cargo or inventory has changed at this checkpoint.

## Production checkpoint

After native/API original CLEAR, the three fixed terminals use the reviewed
shared pattern directly. All16 unchanged original assertions pass, as do42
existing trading controls. Only trading.rs plus its own two inventory digests
change in production; no lib/account/login/candle/book/signing/engine/host path
is adopted. The inventory is trading8->5 and family48->45 on the plan base.
CALLERS records the newly enforced admission/capacity/clock/receipt precedence.
Bounded source review and exact intended-head CI remain required before READY.
