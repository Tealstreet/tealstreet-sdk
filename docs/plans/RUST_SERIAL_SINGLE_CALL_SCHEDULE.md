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
