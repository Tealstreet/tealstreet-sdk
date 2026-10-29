# Independent complete Plans on a login Session root (engine)

Engine-only. Owner: claude-m26ps4. No caller-visible change until a venue adopts
it. Paradex PR-3b is the first adopter.

`admit_session_plan` admits an independent Mutation Plan on a live login Session
root, with each call's auth declared separately (`CallAuth::Session` or
`CallAuth::Preflight`). With it a JWT-bearer venue can run native
continue-on-error writes:
- cancels and cancel-all keep attempting every chunk after a failed or Unknown
  chunk;
- the TP and SL of an accepted parent continue after a failed or Unknown sibling.

Continuing past a failure covers native outcomes only: a failed, refused or
Unknown call. It does not cover a venue's own declaration error.
- **A refused issue ends the Plan.** A refused issue on any call ends the whole
  Plan by the existing rule. A changed resend body is one example, and so is a
  signed recipe on a Session call. Calls already in flight project Unknown, and
  unissued calls are NotSent.
- **The refusal names its call.** The call is counted from 0 in declaration
  order: steps in order, then calls within each step. For example, "Session Plan
  resend changed its request (call 1)".
- **A refused resend stays uncertain.** A refused call whose resend was never
  sent stays uncertain in the journal, as it does for serial Plans.

Every other Plan route still refuses a Session root, so behaviour is unchanged
for every current venue.
