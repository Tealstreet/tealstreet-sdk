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
