# WOO private socket egress

Merge only after deaws confirms that both tunnel hosts have the production
`wss.woox.io/v2/ws/private/stream/` relay allowlist deployed. This SDK/native
change rides the next cut; the local WASM override has no feature flag.

Proxied or HOW-tunnelled mainnet accounts now dial their private WOO stream
through the same fleet as REST, retaining the account's public application ID.
Public, unproxied and staging streams stay direct. The shared native resolver
matches the TS plan; native Hosted WOO still refuses before venue I/O.

Auth diagnostics retain numeric code/status and fixed documented reasons only.
Free vendor message/error fields are deliberately discarded. `-1002` combines
key, permission and expired/revoked-key causes and must not trigger IP advice.
No published IP-specific WOO code is mapped by this change. Existing account
HTTP errors already retain their numeric HTTP status and WOO code. No legacy
endpoint retirement, live egress verification or live auth success is claimed.
