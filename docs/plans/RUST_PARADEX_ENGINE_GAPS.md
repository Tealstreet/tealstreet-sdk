# Paradex engine prerequisites after the signing decision

Audited on actual `77f124089477a6aed502d9b02aa7f08752b7bfa5`.
The hosted provider may receive the minimum Stark signing material. Local and
remote modes remain available; B/C is retired. There is no remaining Sam-only
hold and StarkDigest already exists. The 43 private owner sites split into
seven independent local/config sites and 36 sites needing engine composition
(account 9, history 9, identity 3, trading 15). The remaining five public candle
sites are outside that private set and need separate fallback/poll composition.

1. **Retained-Preflight and Plan Session wait/resume.** session_wait currently
   requires Context phase. Add a reviewed composition for an exact retained
   Preflight or command-owned Plan leg to park on a live Session generation and
   resume with the same original caller bound and exact Plan/Slot/Context/Attempt
   custody. No new command admission, deadline renewal or manufactured Context.
   Waiting before leg activation must not spend the physical 20s window; a
   resumed expired root must neither issue nor publish. Retire/failure fans out
   once and fences late auth answers. This is not permission to widen ordinary
   session_wait or execution-owned Plans without owner review.

2. **Session-fenced just-in-time Bearer headers.** ManagedRecipe currently has
   static unsigned headers or a signed recipe; neither supplies venue-owned
   dynamic JWT headers at prepare. Add a redacted venue hook at actual prepare
   and every reprepare/retry, fenced by exact target/attempt and live Session
   generation. Keep JWT storage in the venue; do not pre-bake queued expiring
   Authorization, invent a SignStep, or copy JWT into engine credentials. A 401
   refresh must produce a new attempt with current Bearer, never resend the old
   packet. Other native headers/body/routing remain exact.

3. **Command-owned Read-Plan history authority (#2011).** Consume a reviewed,
   actual API for exact Plan/Slot/Context/Attempt row staging, atomic page accept
   plus slot settlement, same-Plan DATA/optional INCOMPLETE and early seal with
   an unissued suffix. Retained recovery requires a nonempty accepted prefix
   and engine proof of the latest released Network/physical Timeout while the
   original caller receipt cap remains live. Preparation/provider/native status
   and root expiry are strict. No Plan-as-Read cast, parallel root, numeric
   fallback or venue terminal authority. #2011 is a prerequisite, not a landed
   capability on this base; JWT wait/header composition remains separate.

Assign the first two APIs to Session/signing-dispatch owners jointly; #2011 has
its own Plan/history owner. Native geometry and original assertion REDs must
be reviewed before adopting these APIs in Paradex.
