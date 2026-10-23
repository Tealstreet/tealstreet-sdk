# Deterministic TypeScript unit transport

## Problem and scope

The Phemex client-order-ID forwarding fixture supplies an empty positions list
but leaves `fetchRawPositionModeForSymbol` live. The formatter then reaches the
real positions endpoint. General CI job 120166442133 timed out at five seconds;
the duplicate suite passed, demonstrating dependence on the external response.

This change owns test isolation only. It changes no exchange implementation,
timeout, transport adapter, replay reference, package dependency or production
request policy. Start from landing `fa13c9030ed51e6e0ee0ce0bfaf898cfea55f0a5`.

## Plan before implementation

1. Add a deterministic original-RED control to the existing forwarding fixture:
   replace its transport with a local spy and assert that formatting makes no
   request. The existing missing raw-position-mode stub must fail this assertion
   without contacting a venue. Supply that missing fact explicitly to fix it.
2. Put one deny-and-record mechanism in the safe-cex unit-test setup. Cover
   mockable global `fetch`, `XMLHttpRequest.send` (including happy-dom's
   synchronous child-process path), and Node socket connection attempts as the
   HTTP/WebSocket backstop. Denial must never call the original transport.
3. Assert the recorded attempts during cleanup as well as throwing at the call,
   so production catch handlers cannot turn a network-dependent test green.
   Rearm around Vitest mock reset/restore and retain checks across cleanup.
4. Preserve intentional fixture transport narrowly: the existing WEEX reconnect
   test binds an in-process server to `127.0.0.1` and explicitly admits only its
   exact bound port during that fixture, revoking admission in `finally`.
   No external host or blanket localhost exception is permitted.
5. Exercise the shared guard's denial, caught-error accounting, mock lifecycle,
   exact loopback-port admission and revocation. Run the complete safe-cex unit
   suite and repair only explicit missing local fixture facts it exposes.

## Boundaries and verification

Keep the existing real WebSocket implementation available for the admitted local
fixture. Tests can replace transports with explicit local mocks; the guard does
not approve runtime network access. Arbitrary subprocesses and independent worker
runtimes are outside this setup's guarantee; intercepting XHR closes the known
happy-dom synchronous escape specifically.

Original-RED evidence, corrected focused tests, the complete unit suite, scoped
typecheck/lint and independent source/adversarial reviews precede readiness.
If the broad sweep requires substantial fixture rewrites, report the scope and
keep the narrow Phemex repair independently reviewable rather than widening into
exchange behavior changes. No merge may overtake the assigned #1783/#1784 queue.
