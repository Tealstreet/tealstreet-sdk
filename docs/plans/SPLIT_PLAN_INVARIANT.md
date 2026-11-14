# Split-plan behaviour for multi-write orders

Applies to every native venue path that turns one order intent into more than one write: max-quantity splitting, batch chunking, serial batch rows, attached or separate TP/SL children, scale/TWAP/iceberg children, cancel-replace, and compensating writes such as a protective restore or an endpoint fallback.

## Reference: legacy TypeScript behaviour

The legacy TypeScript adapters define the correct behaviour. It has been tuned over years, and the Rust port does not redesign it. For every path:

- Rust matches what the legacy TypeScript did, including its validation, its write order, and any compensating writes it performs (protective restores, amend-to-replace fallbacks, endpoint fallbacks, algo re-placement).
- Where Rust deviates from the TypeScript, the fix makes Rust match the TypeScript. A behaviour the TypeScript lacked is not added in a parity change.
- Where the TypeScript also places a split order partially, or leaves another known gap, that is recorded as a known issue for a separate product decision, not changed in a port fix.
- After a refused or failed element, Rust does what the TypeScript does for that path: where the TypeScript continues with the remaining writes, Rust continues; where it stops, Rust stops. No shared helper imposes stop-at-first-failure.

## Exception: stop after an unknown outcome

The first recorded exception to TypeScript parity: when an element's outcome cannot be known (an ambiguous venue reply, a timeout, a lost transport after a possible send), Rust halts further writes for that action, even where the TypeScript continued. In that case it:

- returns a clear outcome naming what was placed and what is unknown, with "check Open Orders, then try again";
- records the event in the outcome journal and telemetry;
- never retries or continues automatically, and leaves any retry to the user.

The reason: a missed order is recoverable, while an accidental double position is not. After a definite refusal, TypeScript parity applies as above. Every path that takes this exception is listed with its TypeScript reference.

## Exception: keep the protective put-back

The second recorded exception, decided by Sam: where Rust already restores a protective order (stop or TP/SL) after its replacement was definitely refused, that put-back stays, even where the TypeScript left the position without the old protective order. The rules:

- The old protective order is restored only after a definite refusal of its replacement. After an unknown outcome, the stop-after-unknown exception applies instead, and nothing is restored automatically.
- A venue that requires refusal proof before the put-back (for example BloFin) keeps that requirement.
- A parity change neither removes an existing put-back nor adds one to a venue that lacks it.

The reason: a position left without its stop after a refusal is a worse failure than the extra write. Every path that keeps this exception is listed with its TypeScript reference.

## Outcome reporting

Per-element outcome reporting (`placed`, `applied`, `notSent`, `unknown`) records what happened for whatever writes the venue's TypeScript-parity path sends. The reporting layer never decides which writes are sent, never stops dispatch, and never refuses a plan the TypeScript would send:

- Accepted elements are `placed` (new order id) or `applied` (accepted effect with no new order id).
- Elements the venue definitively refused, and elements never sent, are `notSent`.
- Elements whose fate the evidence cannot establish are `unknown`; a partial intent is never reported as whole.

## Required test shape

Every PR that touches such a path:

- Cites the legacy TypeScript reference for that path (file and line), states whether the TypeScript validates the whole plan before writing, and states whether it continues or stops after a refused element.
- Carries a parity test showing the Rust write sequence matches the TypeScript for that path. Use a scripted transport that records every request, and cover at least one predictably invalid element and one runtime refusal.
- States in the PR body whether the defect was a parity gap (fixed here) or legacy TypeScript behaviour (known issue, unchanged).
