# Command receipt caps for Read Plans

Owner: codex-cxxyqr. Reviewers: codex-uufub4 (Plan core) and
claude-fe7lgm (#1945 receipt custody). Assigned as a separate engine PR after
#1945. Base: actual `a828ede7ebb`. Status: plan only; no runtime changes or
executed REDs. #1945 is not frozen: owner checkpoint `acc3f92b6c9` still needs
receipt-retention/sequence fixes and independent source/host reviews. It is
reference material, not an authorized source dependency. Stack on its announced
frozen head before writing tests or implementation.

## Problem and invariant

`admit_plan_owned` in `request_context_planned.rs` currently computes the
whole deadline as `schedule.bound().deadline(self.now)`. That is the checked
native execution envelope from admission time, deliberately excluding earlier
prerequisite reads. It does not cap work at the caller's original receipt
boundary. Reusing a previously observed `Remaining(ms)` after JWT/preflight
work would start that duration again from the later clock.

For a command opting into receipt-capped Plan ownership:

```
receipt_deadline = deadline captured by #1945 at external command receipt
plan_deadline = min(native_bound.deadline(admission_now), receipt_deadline)
```

The receipt deadline is absolute and immutable. Internal re-dispatch, JWT wait,
preflight DATA, page/stage transitions, retries, re-signing and readiness cannot
renew it. A shorter checked native envelope stays shorter. Native step counts,
concurrency, retry attempts/windows, fallback declarations and per-call bounds
stay unchanged; do not reduce geometry to manufacture a caller cap.

This is a lifetime restriction, not send, retry, completion or history authority.
An opaque Plan/Slot never becomes an original Read root.

## Proposed small API and scope

API shape cleared by uufub4 at `a2795215932`, pending fe7's receipt-fit review:
an explicit receipt-capped Read preflight entry operation (provisional name
`begin_preflight_with_command_wait(policy)`) returning the existing opaque
`Preflight`. It shares the existing genuine-entry transaction and issuer. It
accepts no numeric command id, public deadline, duration, caller bool or
caller-minted token. It refuses a WriteAfterReads policy in this first slice.
The existing native-envelope preflight mode is a separate declared contract,
not a fallback when receipt capture fails.

The exact public signature and whether the mode belongs on the existing entry
operation are review gates. Do not add two issuers or a second Plan allocator.
A missing, foreign, duplicate or expired receipt must never silently select the
uncapped mode or a fresh 120-second default. #1945 owns default stamping once at
external receipt and Fresh-only `reject_expired_command(Operation)`.

The new mode must feed the single existing `admit_plan_owned` implementation.
For command-owned Read Plans it applies the retained cap before any reservation
or irreversible transition. This first slice excludes execution-owned planned
groups and mutation Plans: their founder/observer lifetime and write journal
contracts require separate composition decisions. Reject a capped preflight on
those unsupported admission routes atomically before consuming the preflight,
changing its phase, reserving credit or allocating a group.
Existing mutation and uncapped planned-execution contracts remain unchanged.

The explicit opt-in mode is preferred over automatic changes to every Read
Plan. Native-only existing admission remains unchanged, with no uncapped
fallback on a failed receipt capture. Exact signatures and receipt-fit clearance
remain gates before source.

## Capture and retained custody

#1945 keeps `Io.command_waits` stamps as `(receipt_sequence, absolute_deadline)`
per command id, and `Entry.receipt` identifies this genuine receipt. Its public
`command_wait()` answers only in genuine entry callbacks. `Waits::deadline` is
currently test-only; this PR owns the smallest necessary internal accessor once
#1945 is frozen.

Capture during the genuine entry transaction, before asynchronous work:

1. Require the entry's own live receipt and exact sequence. A duplicate id's
   entry cannot borrow another command's deadline or retire its stamp.
2. Obtain the original finite absolute deadline directly from the engine's
   receipt state; do not reconstruct it from an integer Remaining duration.
3. Bind private receipt identity/deadline data to the newly admitted Preflight
   issuer and command generation. Capture and preflight ownership succeed or
   refuse atomically. Store no independently addressable receipt table here.
4. Retain that data in the existing preflight owner. JWT/preflight callbacks
   use the opaque Preflight plus exact native handoff custody; they do not need
   or fabricate a fresh command entry. Admission from that callback is normal.
5. At Plan admission, validate that exact preflight owner/generation, compute the
   checked native envelope normally, take the minimum with the retained cap,
   and refuse an expired cap before Plan/slot/timer/id/credit allocation.
6. Move the cap into the existing Plan owner; the handle's root and all slot
   windows use the resulting immutable deadline. There is no later lookup by
   command id and no renewal if that id is reused after terminal cleanup.

The original raw deadline, not a rounded-up integer timeout, controls equality
and sub-millisecond boundaries. Use #1945's Expired rule where appropriate; no
`Remaining(0)`, `Some(1)` workaround or positive-time rounding can revive it.
An admission refusal leaves the preflight's valid failure custody intact so the
adapter can settle once through `fail_preflight`. After admission, completion
and expiry use the existing common prepare/commit terminal funnel.

## Preflight boundary and caller effects

This PR caps the resulting Plan. It does not add a preflight-wide timer, replace
JWT request timeouts, cancel transport when a host listener goes away, or invent
recovery for an expired preflight. A native prerequisite already in flight can
finish under its existing bound, but its late handoff cannot admit a new Plan
past the captured receipt deadline. Any stronger prerequisite cancellation or
per-call cap requires a separately reviewed lifecycle change.

The source commit must add CALLERS documentation: opting in may end a long Read
at the caller receipt boundary even when its native page budget remains. A
raised per-call or configured host timeout must be captured exactly, and it
still cannot enlarge a shorter native envelope. Host promise abandonment and
engine-root expiry are separate events; no claim that the former cancels work.

History consumers preserve accepted-only recovery only for their approved
actual page failure while the Plan root is live. Root expiry cannot publish
accepted rows, emit an INCOMPLETE marker or run a native finalizer. No accepted
rows means strict failure; failed staging never enters accepted output. This
PR supplies no Plan-history terminal or page admission API: that remains #1928.

## Tests first, after the frozen dependency and plan review

Use genuine external receipt/decode, Preflight and managed preparation/release.
Keep a working short native-plan control in the same harness. Do not count a
missing API or compile failure as an assertion RED. Separate the following
runnable current-source reproduction from future API-shape tests:

- Receive a Read with a 100 ms caller wait at t=0. Complete an owned prerequisite
  at t=60 and admit a native 100 ms Plan. Current source yields deadline 160;
  expected capped deadline is 100. Assert native geometry remains 100 ms.
- Repeat with default/overridden waits through #1945's real host envelope and
  with a native bound shorter than the remaining caller wait. The minimum must
  choose the correct original bound in each direction.

New-mode controls after implementation:

- JWT/preflight HTTP handoff at t=60 works without an entry callback and cannot
  remint the receipt. A held command's remaining budget is measured from receipt.
- Exact receipt expiry and sub-millisecond remainder refuse admission with the
  approved typed Read Timeout shape, no outcome. No Plan/slots/readiness/timer,
  credit, root or id is added; the retained preflight fails exactly once.
- Duplicate receipt cannot read/replace/clear the original cap. Foreign issuer,
  reused command id/generation and retired preflight cannot borrow a new stamp.
- Two serial slots plus FreshAttempt retry keep one root. Early completion,
  later-slot issue, stage restart and re-sign never move its deadline.
- A final physical timeout before the root expires remains distinct from root
  expiry at equality. Expired root and late DATA are inert for rows/markers;
  the engine's existing expiry funnel emits at most one terminal.
- Root cap preserves the declared number of attempts, concurrency, fallback
  geometry and per-call timeout declarations; no selected peer is retired to
  evade the cap. Rejection is side-effect free for another live owner.
- Unsupported capped mutation/execution admission refuses before changes;
  existing uncapped mutation/execution tests and journal projections still pass.
- Feature off builds and ordinary uncapped Read preflights retain their declared
  behavior; missing receipt never becomes an uncapped success on the new route.

No tests or runtime adoption are claimed in this plan checkpoint. Exact missing
receipt/expiry error projection and refusal accounting are API-review gates.

## API-fit receipt

uufub4 cleared the plan at `a2795215932`: one existing issuer and
`admit_plan_owned`, private receipt cap captured in genuine entry and retained
by issuer/generation, raw deadline minimized with checked native geometry, and
atomic refusal of capped mutation/execution routes. Expired or sub-millisecond
caps yield typed Read Timeout while preserving valid preflight failure custody;
they add no timer or transport rights. This is API-fit clearance only. Source
still waits for frozen #1945 and fe7's receipt-fit review; runtime/source review
remains separate.

## Files, reviews and gates

Expected engine regions: `request_context_entry.rs` (private receipt capture),
`request_context_preflight.rs` / `request_context_preflight_call.rs` (retained
cap and shared entry issuer), `request_context_planned.rs` (one capped admission
path), and focused engine tests. Exact ownership must be coordinated with uufub4
and fe7 before any source writes. Reuse existing bounded command/Plan storage.

No host, signer, venue, history algorithm, geometry constant, inventory or
checker policy changes. No public mutable Cx callback or public f64 authority.
The source commit includes CALLERS; this plan-only PR only adds its index row.

Review order: frozen #1945 preservation, both owners' plan/API-fit clearance,
committed original assertion REDs, narrow implementation, independent source
review, scoped feature-on/off tests, strict Clippy/fmt/bans and exact-head CI.
Consumer adoption needs its own native history and expiry controls. No merge
without the overseer's explicit GO. Architecture gate: No; existing receipt,
preflight and Plan ownership paths, with one additional checked lifetime cap.
