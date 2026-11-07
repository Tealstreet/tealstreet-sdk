# BloFin socket liveness

Both feeds use the engine-managed lifecycle with independent epochs and retries.
Public keeps connect 20 s, idle 20 s, ping 5 s and backoff cap 16 s. Private keeps a 10 s
connection deadline, separate 10 s authentication deadline, ping 5 s and idle 20 s.
Pongs refresh liveness but never extend authentication. Only definitive code 0
login proof enables subscriptions and private financial payload publication.

The unchanged Primary SessionAuth provider signs a fresh login per current
Opened. The signed login is never an interest or replayed on reconnect. Public
interests preserve refcounts and books5 sharing; sequence gaps request managed
repair. Retirement invalidates books before replacement rows. See
`docs/plans/RUST_BLOFIN_MANAGED_SOCKETS.md` and production-binding controls in
`tests/managed_sockets.rs`. Replay tapes are scripted evidence, not live account
connectivity evidence.

Dispatched writes answered by HTTP 429, 408 or 5xx remain unknown through the
engine journal, regardless of a native refusal body. Native code `429` inside
HTTP 200 is also unknown, including a row with order ID `0` under an
`All operations failed` envelope: it cannot prove that nothing was placed or
authorize a protective put-back. Native rate failures direct the caller to
Check Open Orders and retain the venue code/message as diagnostic evidence.
Reads and documented definitive refusal codes on other statuses are unchanged.

Unknown native placement codes, including internal errors and duplicate client
IDs, also remain unknown under an `All operations failed` envelope with order
ID `0`. They cannot authorize a protective put-back. Uncertain placement rows
direct the caller to Check Open Orders; native diagnostics remain available.
