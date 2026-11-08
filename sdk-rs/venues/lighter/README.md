# Lighter placement planning

Before requesting a nonce or submitting a placement, the venue prepares every
native lot and attached child. Pure validation checks the encoded quantities,
prices and triggers against the market grids, size bounds and minimum quote.
Opening market orders obey the same minima. The existing dust-close path is
limited to an unsplit reduce-only market order matching the offsetting position.
Native grouped protection children retain their parent-fill sizing (amount zero).

Splitting balances integer step units across at most 10,000 lots, conserving the
normalized total and respecting each lot's maximum. Ordinary batch writes contain
at most 31 transactions; all chunks validate before the first request.
