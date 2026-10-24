# Hyperliquid fixed local settings and schedule refusals

Plan-first base: actual `9d344d541d5068f6d34f43948d6c65773ab10029`.
The initial checkpoint adds only this plan and a new native test target. Source,
existing tests, Cargo and all manifests remain exact to that base until the
original controls and API/native plan receive independent review.

## Scope and exact count

Nine legacy terminal occurrences are workable without signing, selection, Plan,
Session or other engine changes. Hyperliquid family31->22 is the intended prune:

| Production region | Occurrences | Native behavior |
| --- | ---: | --- |
| trading.rs standalone spot SetLeverage | 2 | 1x completes []; every other value refuses Unsupported/NotSent |
| trading.rs standalone spot SetMarginType | 2 | cross completes []; isolated refuses Unsupported/NotSent |
| trading.rs merged spot leverage or margin | 1 | silent [] success for either setting, regardless of requested value |
| trading.rs SetPositionMode | 2 | hedged refuses Unsupported/NotSent; one-way sets Store hedged=false then completes [] |
| trading/schedule.rs standalone spot ScheduleCancel | 1 | Unsupported/NotSent before time or credentials |
| trading/schedule.rs invalid deadline | 1 | InvalidInput/NotSent before credentials |

The shared merged-spot success line serves two commands; it is one scanner
occurrence. The one-way mode applies to perps and spot and keeps its Store
projection. Standalone spot's leverage/margin branches precede market lookup,
while merged spot classification still uses the existing Store market. Preserve
that precedence, including native no-op values that would be invalid on perps.

Current combo5b ownership was confirmed by auditor8x against
train/combo-5b-3573 at989603e75a4 (#1831 refrozen2094). No member touches ANY
Hyperliquid path. Older train/combo-5b and train/combo-5b-be71 are superseded.
Only our exact two HL manifest keys may shrink when source is adopted. The plan
and new target do not edit shared indexes at this initial checkpoint.

## Existing APIs and terminal authority

Use done_local_setting at the four local success lines, with exact [] result.
SetLeverage, SetMarginType and SetPositionMode are genuine Intent::Setting
entries in Command::write_intent. No-op closures make no Store change; one-way
mode's closure alone projects set_hedged(false). The shared prepare/commit local
setting transaction must refuse before that projection and own one terminal,
without a Context, credential, journal, HTTP, timer or numerical fallback.

At the five native refusal lines, acquire genuine Mutation command admission
with unsigned20s/Never policy and fail_context on that exact owner, preserving
the native VenueError message, kind and NotSent outcome. Admission rejection
already emits the single shared refusal; return without another native failure.
No new local admission helper or policy classifier. Shared refusal metadata takes
precedence over the native validation only when admission itself refuses.

A full command/context registry must therefore refuse before local success,
Store projection or native refusal. LocalSetting's owner capacity is distinct
from the Context pool: capacity controls retain256 genuine Command Read owners,
not only Background Contexts. Retire an exact peer through its own Context to
show capacity recovery while the other255 remain owned. Duplicate command ids
must preserve the original owner and produce no replacement completion.

## Native behavior to keep

Schedule validation remains: spot refusal first; perps deadline must be a finite,
nonnegative safe integer UTC millisecond value and at least5000ms after native
cx.now. None is a valid disarm request and remains on the signed path. Retain
both exact validation messages: time < now+5000 refuses, equality is accepted, with no projected
clock or synthesized signature expiry. The existing credential checks, including
missing-key behavior and precedence, stay byte-exact and unconverted.

Exclude all real writes, sign/dispatch/crypto/setup/selection, raw-key checks,
Place/Cancel/Amend validation, minted cloids, job queues, transport, native ACKs,
reconciliation, financial reducers, socket/History/Builder and all lib.rs bodies.
No valid request or native signing byte changes, and no new grant or retry.
The shared local completion is used only for the existing synchronous successes.

## Original compiling controls

Use a real Engine with a thin Venue delegator for genuine retained peer owners
and fixture Store setup. Native commands execute the unmodified Hyperliquid
implementation; no fabricated Context or synthetic HTTP/signing result.
Separate full-capacity cases for every native local path, including both merged
commands sharing one success line. Each first asserts one typed Capacity failure
with Mutation NotSent, no native fallback, no IO or Store change. Recovery and
peer-preservation suffixes are reported as gated until that first assertion is
green. Working controls pin native messages, Unsupported versus InvalidInput,
[] results, one-way Store behavior, spot/merged precedence and safe deadline
boundaries. Invalid-clock, duplicate and caller receipt entry controls are
separate from venue-native defects.

A missing API, compile failure, fixture setup mismatch or ignored assertion is
not RED evidence. Record first failing assertions and gated suffixes honestly.
Use CARGO_BUILD_JOBS=4 and only one scoped local build. CI owns workspace,
all-feature, WASM and replay/performance validation. Commit originals before
source; request native/API reviews before production. The overseer's continuation
ruling permits production after reviewed REDs, with draft PR and CI parallel to
source review; no production is included in this checkpoint.


## Original execution receipt

Plan ffcb9d13b17 precedes the new request_context_local_settings target. Fe7's
bounded API/native plan review is CLEAR, and9cz's native plan review is CLEAR.
The exact native boundary is folded: time < now+5000 refuses, equality passes
validation. A working control pins one millisecond below, exact equality and
None disarm; the latter two reach the unchanged missing-key refusal, without
performing or modifying key selection or a valid signed request.

On unchanged actual9d344 production,25 compiled tests execute12 intended assertion
REDs,13 GREEN and zero ignored. Ten separate full-capacity cases cover all nine
lexical sites (the shared merged success has separate leverage and margin cases).
Five success cases still complete [] in the full pool; five refusal cases still
emit their native Unsupported/InvalidInput instead of the shared Capacity kind.
Two invalid external-clock cases expose one-way success/projection or native spot
schedule refusal instead of the shared InvalidInput. All ten native behavior
controls, the exact duplicate-event/original-peer control, invalid caller-wait
entry control and deadline-boundary control are GREEN.

Capacity recovery, exact first-peer retirement and post-recovery native outcomes
are suffix assertions behind each first Capacity RED. Store preservation under
capacity/invalid clock is also gated after those first assertions: no executed
proof of the future refusal projection is claimed. The working native tests do
prove current exact [] results/messages/kind/NotSent, non-I/O behavior and the
one-way Store change. No ignored tests, missing APIs or compilation failure are
counted as REDs.

Two fixture corrections precede this receipt: native metadata resets the hedged
flag, so the explicit test-only seed is applied after metadata; the common
engine duplicate diagnostic is Notice::Event(COMMAND_DUPLICATE), not a
command-scoped CommandError. An unsuccessful text-patch assertion left the old
fixture unchanged and its rerun was discarded. Only the final25-case target is
evidence. No production, Cargo, existing fixture, checker or manifest changed.

Scoped strict Clippy on the new target and scoped rustfmt/diff checks pass, with
four cargo jobs and sequential local builds. No workspace or all-feature sweep.


## Retained-peer strengthening before production

Both original reviewers CLEAR26e34250300. Fe7 independently ran13 GREEN /
12 assertion REDs;9cz's review is read-only. The requested P3 now validates each
of all255 remaining peer Contexts after exact first-peer retirement and recovery,
then checks the observed count. The original25 controls still execute12 intended
REDs/13 GREEN/0ignored on unchanged9d344 source; the new suffix remains gated
until cutover. No direct next_id introspection claim is made by integration tests.
