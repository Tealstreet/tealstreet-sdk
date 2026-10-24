# Aster frozen-startup clock assertion boundary

Test-only plan, codex-ih35js, 2026-10-24. Base actual landing
`be710b3f2d7692244144b3f8f10810dc52790796`.
No venue, production clock, replay executor or request validator change.

## Evidence

Foreign Coinbase PR#2036, exact head2acfca8d3f0887c1af776a2527e14514d0e939c0,
Rust SDK dispatch40088224529, BingX measurement job120855396800, fails
asterdex-clock.test.mjs:171 in the frozen-startup/readiness test. Expected
1760880001500, observed1760880002500 after6550ms wall execution. Child status,
sync probes, real signed-order timestamps and real timestamp validation already
passed before that assertion. Source clock/replay/API inputs are byte-identical
to9330 and this landing. Focused Node22 test on9330 passes4/4, including the
no-reset and rebase negative mutations.

ReplayClock deliberately advances frozen startup to armed native timer deadlines.
Aster axios rate-limit windows arm1000ms resets even below their request limit.
Wall-load can deliver a reset while Tape.play waits for startup readiness. The
manual1500ms post-sync probe is therefore a lower bound, not the final ready time.
An independent actual1000ms timer probe yields before1760880002500, correctly
rewinds to1760880000000 at start, then advances tape600 to1760880000600.
Independent source review agrees; the overseer rules this test timing-flaky.

## Bounded change and RED sequence

1. Commit this plan before the test changes.
2. Add an opt-in fixture parameter that awaits a real1000ms startup timer after
   the existing1500ms post-sync probe, inside the isolated child. Keep the clock,
   signer, sync method, route and validator real. Add a fifth test using it.
   With the old fixed readyBefore assertion this must fail deterministically;
   existing four tests remain identifiable and keep their assertions.
3. Assert readyBefore is a safe integer and at least the captured post-sync
   boundary (and the observed awaited timer boundary for the new control).
   Keep exact readyAfter==epoch, before/after sync probes, exact signed order
   timestamp/now, all request-validity checks, both negative mutation causes,
   replay invalid/unserved/unused counts and command result assertions.
4. Run the one changed Node test file, one local process at a time. No cargo,
   local WASM build, performance sweep or load generator. Scoped formatting and
   independent source/caller/full-map review; send the immutable delta to the
   affected Coinbase owner for bounded review. Push a small draft PR and CI.
   No merge without an explicit overseer window.

Architecture gate: **No**. This corrects a harness assertion about the existing
clock contract. It neither changes scheduling/signing policy nor relaxes the
independent wire validator. Product code, TS venue bytes and Rust inventory stay
exact. This is not a clock/runtime fix or a CI waiver.


## Executed regression and focused verification

Plan committed first at `299c636614b`. Original control commit `0d097c39e9c`
adds the awaited native 1000 ms timer while retaining the old equality. Its
isolated test fails at the intended assertion: expected 1760880001500, observed
1760880002500. It passes through real replay, signing and timestamp validation
before reaching that assertion.

The implementation uses the observed post-sync boundary, or the captured timer
boundary for the added control, as a lower bound for `readyBefore`. The exact
rewind and all existing signed-wire, validator, mutation and replay-result
assertions remain. Node 22 focused execution of the changed file passes all five
tests, with zero failures or skips. Scoped Prettier and whitespace checks pass.
No Rust or WASM build is needed for this test-only change; CI remains required.
