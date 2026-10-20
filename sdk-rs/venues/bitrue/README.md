# Bitrue socket recovery

Public and private sockets use independent engine `Supervisor` instances with
10-second connect/token deadlines, 20-second receive watchdogs, and capped
exponential reconnect with deterministic jitter. Opening a socket does not
reset an unstable session's failure count.

Heartbeats remain server initiated. Public replies preserve the existing
literal and numeric pong forms; private replies preserve the server timestamp.
Each private recovery POSTs a fresh futures listen key. Session generations
fence late responses and cancel old keepalives. Public reconnects retain
subscription ownership, retire pending socket reads, and invalidate books.
