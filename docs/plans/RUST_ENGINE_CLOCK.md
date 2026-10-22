# Engine clock standardisation

Owner: codex-5c57jl; base `e42a32a8e945`; owner directive 2026-10-22.
Clock policy is cross-cutting. Fix the shared engine clock ingress rather than
adding another venue opt-in. Signing core and Binance belong to codex-uufub4;
its dispatch preparation continues to consume `Cx.now: f64` epoch milliseconds.

## Contract and phases

1. Audit clock ingress, existing venue calibration and tape-clock injection.
   Production SDK engine transports use the shared synchronized clock before
   startup and on every later event. Explicit host/replay clock providers remain
   authoritative, dynamic and common to commands, HTTP/WS completions, timers,
   nonces, expiry and signer dispatch; never overwrite an injected tape clock.
   All venues consume the same engine event-time contract without name lists.
2. Retire `host_clock` / `hostClock` as a per-venue policy and remove its native,
   SDK and fixture opt-ins. Preserve binary flag positions: reserve former bit14;
   never shift later fields. Update capability/frame tests and nearby specs.
   Re-verify venue server-time correction paths; retain protocol-specific parsing
   but route any shared clock policy through the engine boundary. Document any
   justified exception explicitly; no new per-venue clock implementation.
3. Regress formerly unsynchronized venue families and former opt-ins through
   startup, commands, WS authentication, HTTP/sign callbacks and timer deadlines;
   prove resynchronization is read dynamically. Regress injected tape clocks even
   when synchronized wall time differs, plus append-only capability ABI. Prove
   failures before fixes and keep all earlier assertions/financial identity proof.
4. Independent correctness/adversarial review must explicitly answer whether this
   is a per-venue fix of a cross-cutting concern (answer: no, one shared ingress).
   Focused native/host/SDK tests, typecheck/lint/Clippy/fmt/bans locally (four Cargo
   jobs); CI owns builds, all family replay/signature parity and fixed budgets.
   No fixture, comparator, allowance, timing or budget relaxation. Integrate any
   relevant actual core/landing change, fresh CI, separate exact revert guard,
   merge green reviewed head, verify actual tree/parents and report the pin.

## Coordination and limits

No change to the `Cx.now` accessor or primitive Signer clock stamping: signer
primitives sign exact bytes and do not sample independent wall time. Coordinate
shared capabilities/SDK/host files with codex-uufub4; engine request/dispatch and
pending-sign logic remain in its core. Ask for a merge window if clock work needs
engine dispatch changes. No venue signer migration until actual signer core.
No TS deletion, default engine flip, publishing or NX edits.
