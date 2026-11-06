# Phemex protective edit post-cancel failures

A protective cancel/create edit records a proven terminal cancellation before
checking whether its replacement can proceed. If a local rate-limit pause or
unreconciled fill prevents the replacement, the command reports typed
`Protection::Unprotected` with the removed-stop warning and instructions to
re-place it. A pause after cancellation has `Outcome::Unknown`, `rateLimited`
and the remaining retry delay; it never says the whole mutation was not sent,
restores, or resends.

Native sparse executions retain their symbol alongside the order identity.
Amend admission, the final protective cancel claim, and the post-cancel fence
check the edited symbol and original order. A fill on another symbol no longer
blocks a native replacement. Unknown or overflowed fill evidence remains a
conservative account-wide veto. A partly filled standalone stop is refused
before cancellation.

VENUE_STATE remains byte-exact version 1. Frame-fed accounts still receive its
account-wide fill boolean, so **a fill on another symbol can still abort their
replacement**. After a proven cancellation, that abort now reports typed
`Unprotected` with **re-place** wording instead of silently dropping the
replacement. The regression exercises both same-symbol and other-symbol source
fills. A scoped source format is a separate, consumer-first rollout after this
PR lands and requires overseer approval before any v2 source ships.

The existing selected-field trigger receipt (at most 5 seconds old, equality is
crossed), unchanged-order, unfilled-order and protected-position checks remain
at the final cancel claim. Cancellation must be terminal before any create.
Only a definitely refused replacement can attempt the original standalone
order once, at its original venue size and type. Attached, grouped or split
shapes never guess a put-back. Unknown sent outcomes never restore or resend.
No engine, public API, source codec, endpoint or host change is required.
