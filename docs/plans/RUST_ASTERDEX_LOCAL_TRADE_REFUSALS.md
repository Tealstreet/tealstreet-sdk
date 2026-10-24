# AsterDEX local trading refusal custody

Owner codex-ih35js, plan first on actual landing
`a7f04339c59227e2cdde07c0d1dad835110d2057`, branch
`feat/rust-aster-local-trade-refusals`. Plan and original assertion REDs only
until independent review. Existing #1987 stays frozen; no Phemex history/income
work before #1974 lands. No merge without an overseer window.

## Scope and exact map

Three synchronous fail sites in trading.rs start_trade remain workable. The
fresh open-PR file audit finds no trading.rs claim. #1987 holds history/lib;
#2013 holds sign/request tests; account/session and asymmetric fixture work
stay with their owners. This slice changes neither those files nor shared
fixtures. Valid remote jobs, signing, queue advancement and async terminals
remain native and inventoried.

| Local refusal | Fingerprint | Planned removal |
| --- | --- | ---: |
| Required credential/auth shape | `e2f7439f01b028bfc35d2a2433f63a73448c83dad981d0d281f24deb4059167b` | 1 |
| Acknowledged placement routing capacity | `2f0770d2ea5f5246cd2ddcd35b614bce738e564c2d95193ff5a84eb8479811e5` | 1 |
| Failed synchronous job construction | `fbafcc48aaedbe70a22d054c16acc0858b261af3d5f711681c39d0e866da7e30` | 1 |

The actual scanner/manifest at this base agree: family43F/67O, trading7F/7O.
This future removal-only prune is3F/3O: family40F/64O and trading4F/4O. If
#1987's disjoint10-occurrence prune lands first, family38F/57O becomes35F/54O.
No manifest edit belongs to the original test checkpoint. No lexical rehash,
macro disguise, checker exception or new legacy caller is permitted.

## Explicit operation and pre-admission planning

FetchStpMode enters fetch_stp_mode/start_trade as Read, including V1's unsupported
error before a Job exists. Every on_trade_command write enters as Mutation.
Pass that operation explicitly through the private start_trade call; never infer
it from a successful first step or default an Err to Mutation. Keep operation
separate from selected signing role: this slice performs no signed submission.

Synchronous job construction, native credential/auth validation and pure capacity
projection may happen before admission. Preserve native precedence: plan error,
then credential/auth refusal, then acknowledged routing capacity. Both existing
CommandError diagnostic and local error message/kind remain in native order
after admission. Mutation local failures remain NotSent. Read local failures
have outcome None; V1 FetchStpMode's legacy NotSent is deliberately removed.
No credential is attached to local failure custody, and credential-free auth
errors supply no auth-health evidence under the shared engine rule.

The capacity predicate must project the retained acknowledged count immutably
using the native Store/terminal predicate, plus existing live job reservations
and the new job's place count. Do not execute acknowledged.retain to discover a
local refusal. Admit the genuine operation first, then settle only that Context;
Rejected already published its terminal, Revoked grants nothing. Failed entry
must leave acknowledged, jobs, native reservations and Store untouched. An
admitted routing-capacity refusal may then run the same native cleanup before
its diagnostic/failure; the other two refusal branches retain their native
no-cleanup ordering.

For a valid native remote job, leave the existing cleanup/insert/advance path
without a newly issued managed root. This is a refusal-only cutover, not partial
remote ownership: no Context may be issued then forgotten for legacy HTTP. A
pure local-refusal decision may share formatting; it must not wrap the engine
issuer or add a local journal, timer, retry or numeric fallback. CancelSymbolOrders also prunes acknowledged during planning, before start_trade.
Its planning branch must project the same retained entries immutably, and carry
native cleanup intent to the admitted local-refusal or accepted native job path.
A failed entry must not mutate that earlier cleanup either. Preserve Store-first
then retained-ack cancellation routing without duplicate IDs. Response-side
acknowledgement cleanup remains outside scope.

## Caller-visible contract

On a valid entry, local diagnostic text, AuthInvalid/InvalidInput kind and native
precedence stay exact; Mutation remains NotSent. FetchStpMode is a Read even when
unsupported or missing credentials, so its local failure has no money outcome.
Invalid clock or full genuine owner capacity instead emits the one shared typed
InvalidInput/Internal admission refusal, before any diagnostic or cleanup.
Read admission outcome is None; Mutation admission outcome is NotSent. Genuine
live duplicate IDs keep the existing owner and diagnostic. No missing receipt
fallback, extra success or signed capability is introduced.

CALLERS must record those outcome/admission/auth-health changes before runtime
adoption. Existing SDK STP/settings/order callers and remote payloads remain.
The remaining four trading inventory witnesses (two async numeric terminals
and two conservative matches macros) are not converted or claimed retired.

## Original REDs and working controls

Commit this plan before tests. Use the real Engine and genuine retained Read
peers for capacity; a wrapper delegates native Aster command execution and
retains only test peers, with no fabricated Context or private issuer checks.

- Independently compile/assert invalid-clock and full-capacity refusals for
  Mutation plan failure, Mutation missing credentials, placement routing capacity,
  Read V1 unsupported STP and Read V3 missing credentials. Check one typed terminal,
  correct Read/Mutation outcome, no diagnostic/I/O/timer/Store change, peer liveness
  and real peer retirement/recovery.
- Pin V1/V3 native diagnostic/error/precedence controls, Mutation NotSent and
  current Read NotSent baseline separately from the intended Read outcome RED.
- A cfg(test)-only child module may inspect a pre-seeded acknowledged entry while
  still executing the real Engine callback. A retired native acknowledgement
  plus an oversized placement reaches capacity cleanup; invalid/capacity entry
  must not prune that routing state. Seed fixture state explicitly, never claim
  that this state seed is a released HTTP/provider receipt. Add a native working
  cleanup control so the negative diagnoses the guard, not setup.
- Preserve valid V1 settings POST and V3 STP GET wire/slot/signature through the
  actual installed provider and native clock response. Full owner capacity must
  not turn otherwise-valid legacy remote jobs into abandoned managed commands.
- Exercise duplicate custody and more than256 synchronous terminal reuses. Aster
  plan_place does not generate UUIDs; do not transplant MEXC entropy-preflight
  claims. Preserve existing native financial tests.

Only test files, a cfg(test) module declaration and this plan/index may change
before review. Runtime production and inventory stay exact. Count only executed
assertion failures as REDs, not compilation/fixture errors; no ignored tests.
Three independent source/adversarial, caller and full-map reviews answer the
architecture question before implementation. Native/strict/checker gates use
jobs2; family WASM/replay/CPU/memory gates belong to CI.

## Family bytes and size gate

Aster's fixed family ceiling is1,343,488 bytes; do not change the ceiling or
speed profile. Prior exact #1987 CI was1,350,073 bytes. fe7lgm's same-toolchain
local measurements were1,350,743 for original f636,1,349,210 for lean8fc,
1,346,890 for lean plus #2004, and1,343,477 for lean plus #2004 plus the
#2006-equivalent read-marks Vec. The last is11 local bytes below the ceiling;
prior CI/local calibration suggests roughly680 bytes of CI margin, not a fresh
measurement or a size PASS. #2004 is actual; #2006 and lean #1987 composition
must be pinned before runtime size certification.

This plan/test-only checkpoint adds no shipped runtime, so its production source
delta is zero. Obtain exact current CI family bytes and pin/run provenance; do
not invent a current size from the historical calibration. Final adoption must
report exact bytes and delta both against its actual tip and against the combined
lean #1987/#2004/#2006 tree. Three lexical removals are not a per-site byte cost.
No local WASM build, venue issuer wrapper or profile change is authorized.

Architecture gate: No per-venue replacement of a cross-cutting concern. Native
job/refusal planning consumes the shared genuine-entry/Context terminal APIs;
operation is explicit, acknowledged capacity is a native pure projection, and
all lifetime, rejection, auth-health and completion authority stays in the engine.

## Original execution and byte evidence

The initial fixture build found an incorrect STP enum variant and an attempted
mutation through Engine's private venue field. Both were corrected before
counting REDs: fixture state is seeded before constructing the real Engine.
The initial executed target gave18 assertion REDs/6 working controls; the three
Read outcome cases are subsequently split into independent tests so each reaches
its own intended assertion instead of stopping at the first loop failure.
Native metadata/precedence,300 synchronous reuses, duplicate custody, selective
ack cleanup, accepted V1 POST and accepted secondary V3 STP GET work at baseline.
Accepted remote wire controls run with256 genuine retained managed peers.

The historical CI browser-wasm-families artifact12543180475 from green landing
Rust run40048277914 pins sourceHead `f8163b7b8523c62fe905894d395405db778140ca`.
Its manifest and downloaded Aster WASM independently agree on **1,321,596 B**,
SHA256 `9484e1a4db2d57b6acc260d97e7698c3cbbfcded5c175c23eff1f4824b9ce9c9`,
leaving21,892 B under1,343,488. #2004 is included; #2006 and lean #1987 are not.
The plan base has identical engine/crypto/Aster/Binance/registry/WASM/Cargo
inputs to that source pin. This is pinned baseline evidence, not fresh exact-head
CI or a forecast for the complete adoption. fe7lgm separately measured
1,322,304 B locally at2f5105596db. No current baseline number is added to the
older lean/full-composition calibration to fabricate an adoption size PASS.

The complete original checkpoint executes **20 assertion REDs /7 working
controls /0 ignored** on unchanged production at a7f04339c59. Fourteen entry
controls cover seven Read/Mutation/auth-shape paths at invalid clock/full peer
capacity; three independent Read outcomes, one auth-health and two pre-admission
ack-state assertions complete the20 REDs. The two ack failures directly show
retired routing entries being pruned before entry is refused. The seventh native
positive admits9,999 placements with one live and one Store-observed ack at full
managed capacity, proving only the live ack consumes native capacity.

Invalid/capacity controls currently stop at the native diagnostic-count mismatch;
their subsequent typed fields, peer validation, real retirement/recovery and
Store assertions are adoption gates, not claimed executed on the failing native
branch. Each independent Read outcome assertion executes and observes legacy
Some(NotSent) versus required None. Existing native fixture and runtime files
remain exact except the four-line cfg(test) child-module declaration.

Scoped Clippy/fmt passed before the final added near-capacity working control;
a refresh follows that addition. Checker controls pass15/15. Initial provenance
failed solely because a newer incoming KuCoin prune was missing from the base;
this is not waived, and a genuine updated-tip merge follows the original RED
commit before claiming a provenance pass.

A further pre-source audit found the earlier CancelSymbolOrders acknowledged
retain in job construction, not only start_trade's capacity cleanup. Its pure
planning and deferred cleanup intent are included in scope, without adding a
fourth inventory removal. Dedicated invalid/capacity state REDs and a native
Store-first/no-duplicate cancellation wire positive now pin that earlier boundary.
The original near-capacity and routing-state controls remain unchanged.

Expanded original target after the CancelSymbolOrders correction executes
**24 assertion REDs /8 native working controls /0 ignored**. The added four REDs
cover invalid/full entry through missing-key symbol cancellation and both
earlier routing-state preservation checks. The added positive pins Store-first
DELETE orderId routing and exactly one remaining live-ack step at full managed
capacity. The earlier27 test bodies are retained except fixture setup support
for the additional case. Existing trading/admission/security targets independently
pass40 tests across3 targets, with all original assertions unchanged.

The first updated-tip ratchet pass was followed by a full-bans provenance failure
on another newer Bitrue prune. Keep that failure visible and integrate its actual
landing before the final provenance pass; no checker override is used.
