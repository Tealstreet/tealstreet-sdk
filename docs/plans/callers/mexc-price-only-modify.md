# MEXC full-position TP/SL amendments

SDK, NX and server callers may amend the price of a native full-position TP/SL
without supplying an amount. Its zero quantity is a full-position sentinel, not
a replacement below the contract minimum. The replacement resolves the current
position quantity through the existing native full-position placement path.

If the matching position is not available after an account reload, the amend
refuses as `VenueUnavailable` / `NotSent` before cancellation. The existing
protection stays unchanged. Callers may retry after position data arrives; this
change adds no automatic wait, retry or background read.

Regular and fixed-quantity replacements retain their minimum checks. Explicit
amend amounts remain total native contracts, including when one contract's base
size is less than one. A zero regular-order amount is never treated as omitted.
