# Lazy per-user idle release

Affects the native exchange server and the NX owner router. The frozen HTTP
contract and future transition/handoff boundary are in
`sdk-rs/hosts/server/DESIGN.md`, under “Lazy owner moves and idle-release lease”
and “Future: long-lived job handoff”. No job migration is implemented.

The router must hide `/proof/*` and `/admin/*` on the public edge, use an Access
service token to request `{userId,requestId}`, and preserve the same UUID on the
single timeout retry. Accept proof only with checkedAt within 5 seconds and
expiresAt at least 5 seconds ahead, and durably commit ownership before the
5-second expiry margin. Otherwise retain the source owner. Repeated proofs
retain their original timestamps; they do not extend the lease.

Idle release never stops jobs. Operator drain and explicit force remain separate.
No external store, release/pin, secret, or deployment prerequisite is introduced.
