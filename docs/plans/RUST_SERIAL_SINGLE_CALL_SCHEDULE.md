# Checked single-call schedules for serial Plans

Owner: codex-ih35js. Engine reviewer: claude-fe7lgm.
Local base: Bitget slice 3, `546f585f187` (tree `965e1d31442`).

The serial admission and continuation paths each declare one call without a
fallback. Previously both used the general step-tree constructor. The composed
Bitget module retains that constructor only through those two paths; the
admission path is inlined into `BitgetFutures::drive`.

`Schedule::from_single_call(CallTerms)` checks the same one-call PlanBound and
constructs the same root sequence, call ordinal, issuer, structural credits and
initial availability. Its input has no fallback field. The general constructor
remains the oracle for the differential tests and serves declared Plans.

With `serial-plans`, the shared admission body accepts prepared Results for the
Schedule, terminal ordinals and optional barrier normalization. It consumes them
in their old order, after the same origin, policy, credential and receipt checks.
Preparing the declaration allocates storage but grants no authority or engine
resources. Without that feature, the original declaration preparation stays at
the same point. All admission checks, resource reservation and commit logic
remain shared; the specialization keeps its cost out of non-serial families.

Serial entry preserves its existing order: the one-Plan-per-Preflight check,
`validate_call`, then origin and policy validation. Thus an invalid serial call
still returns Invalid before a revoked Preflight, whereas declared admission
returns the origin or receipt error before an invalid Schedule. The patch does
not move `validate_call` or change the continuation's custody checks.

The S1 change here is limited to constructor plumbing in `continue_serial`;
there is no method erasure or change to its P-free boundary. The other S1 methods,
S2 flush loops, socket route work, journal projection and #2110's note handling
are outside this cut.

## Verification

- Differential constructor controls span timeout 0/1/u32::MAX, attempts
  0/1/2/255/256/257/u16::MAX, Shared/FreshAttempt windows and waits
  0/250/u32::MAX. They compare errors, bounds, layout, issuer isolation,
  start/retry/settle/cancel and unused-credit release.
- Genuine command-entry controls preserve serial and declared precedence for
  live, retired, receipt-expired and already-Planned owners. A refused admission
  spends no IDs, Context storage or actions.
- Existing serial, condition, declared-plan and mutation-barrier controls remain
  required. No retry policy, timeout, step cap, evidence or native wire change.
- Measure shipped Bitget and Bybit before and after on this exact composition,
  using unchanged harness profiles and Binaryen 132. Credit measured bytes only.
  No budget or optimization-profile changes belong in this PR.

Paired shipped measurements on the same `546f585f187` composition, Rust 1.99.0
and Binaryen 132 (Web and Node agree):

| Family | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Bitget | 1,472,027 | 1,470,088 | -1,939 |
| Bybit | 1,396,213 | 1,396,216 | +3 |

The raw Bybit module decreases by one byte; its shipped module increases by
three. Credit is limited to these measured results. The overseer accepted that
Bybit increase as noise against its available margin. No allowance is changed.
The general Schedule/PlanBound constructors and terminal-ordinal walker are no
longer retained by Bitget. Bybit retains its declared-Plan implementation.

The exact production delta received fe7lgm's source CLEAR at tree
`8744b9c797f7`. Scoped engine controls with serial Plans and mutation barriers
pass 1,157 tests; the serial-disabled controls pass 1,089. Both retain 10
existing ignored tests. Strict Clippy passes with serial/barriers enabled, with
serial disabled, and with serial disabled plus barriers. Scoped formatting
passes. Full bans currently report foreign Backpack inventory drift from the old composition;
actual-tip integration must recheck that ratchet before a PR is ready.


## Actual slice-3 landing composition

Plain-merge of actual `ae3f8b47cc1` into the reviewed cut gives source
`8ae3b21200c`. The only manual source hunk retains #2093's incoming
`nonce_unscheduled` refusal in the shared `has_barriers` check. The reviewer
carried exact-patch CLEAR; `validate_call` and all other own source deltas are
unchanged. README keeps the incoming rows plus this plan row once.

| Family | Before on ae3 | After on the same composition | Delta |
| --- | ---: | ---: | ---: |
| Bitget | 1,475,547 | 1,473,607 | -1,940 |
| Bybit | 1,397,889 | 1,397,892 | +3 |

Rust 1.99.0, Binaryen 132 and unchanged profiles; Web and Node agree. Raw Bybit
shrinks by one byte, retaining the overseer's accepted three-byte shipped
tolerance. No allowance change and no unmeasured sum with cut 2.

Fresh composed engine library rows pass 1,232 serial/condition/barrier/nonce,
1,155 declared/nonce, and 1,167 declared/barrier/nonce tests. Each retains the
same ten existing ignored tests. Strict all-target Clippy passes all three
rows; scoped formatting and full bans/ratchets pass. The incoming unscheduled
nonce barrier refusal and ordinary-plan positive controls run in these rows.


## Resumed landing-tip verification

Plain-merge of actual `34999f973f9` gives measured source `285801cd67f`.
README keeps incoming rows plus this plan row once; every own SDK source and
test blob remains exact to the green reviewed head `38788da3380`. The only
incoming engine change is the independently landed socket helper.

A fresh same-tip pair retains Bitget 1,475,547 -> 1,473,607 (-1,940) and Bybit
1,397,889 -> 1,397,892 (+3, the accepted tolerance). Web/Node agree and the raw
Bybit artifact still shrinks by one byte; all shipped hashes match the prior
same-source measurements. Rust 1.99.0/Binaryen 132 profiles are unchanged.
The completed measurement target is removed and receipts/artifacts retained.

Fresh focused controls pass 42 serial schedule/admission cases, 37 declared
schedule cases, and both nonce barrier/ordinary positives in serial-enabled
and serial-disabled barrier configurations (two each). Strict all-target
Clippy passes serial and declared rows; formatting and full bans/ratchets pass.
The qualifying full Rust run 40200956576 and general run 40200956635 are green
on `38788da3380`. Current-tip integration requires the auditor's composed-tree
check before publishing/landing; no source edit is hidden behind this carry.

The subsequent AsterDEX landing `aec49a904bc` merges automatically. It changes
no engine, host, registry or Bitget/Bybit source input. The README correction
preserves every incoming byte and appends only this row plus one separator;
`git diff 34999f973f9 HEAD -- docs/plans/README.md` removes zero tip lines.
