# OKX trailing stops are drawn and edited as levels

A trailing stop's row price is its trigger level, where callers draw and drag
its line: OKX's `moveTriggerPx` once published, otherwise the callback distance
behind where it trails from (its `activePx` while activation is pending, else
the current last). A caller sends a trailing price as a level; the venue turns
it into the distance from a last price under 5 s old. After activation, a drag
re-places the trail at the distance between that fresh last and the dropped
level, so a drag after a pullback shrinks the trail. A price edit of a stop
whose activation is pending is refused before any cancel, since it has no trail
to measure from yet; re-place it instead. Size-only edits are unaffected.
