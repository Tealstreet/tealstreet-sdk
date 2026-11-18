# Bybit balance.info from REST only

**Change.** Bybit no longer merges `wallet` topic pushes into `balance.info`.
`info` is the last REST wallet account, which the 60 s safety reconcile
re-reads while the private topics are live. The typed wallet and coins (total,
free, used, upnl) still stream from the topic.

**Why.** #2494's merge used `serde_json::Value`, which check-bans forbids, and
pushed the bybit wasm past its 1,453,056-byte cap. Every Value-free merge cost
about 5 KB, against about 1.2 KB of headroom. All sizes are local, binaryen 132:

| Build | Bytes |
|---|---|
| master | 1,462,992 |
| before #2494 | 1,451,676 |
| raw-value map | 1,468,692 |
| serde visitor | 1,456,923 |
| byte scanner | 1,457,274 |
| splice | 1,456,415 |
| this change | 1,451,855 |

**Callers.**
- SDK: `balance.info` (accountType and the coin details) can lag a push by up to
  60 s while the topics are live.
- NX: none.

**Obligation.** Post-cutover follow-up (ddi): merge the pushes into `info` on
the TS side, outside the wasm.

**Deletion prerequisites.** None added.
