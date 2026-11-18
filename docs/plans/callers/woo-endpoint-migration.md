# WOO endpoint migration: proposed caller gates

Proposal only, following #2981/#2982. No behavior or activation changes in this PR.
The [native inventory and RED plan](../RUST_WOO_ENDPOINT_MIGRATION.md) covers
26 REST operations and two socket families; one fill-history operation is already
current V3. SDK/native-server/NX callers must wait for the implemented, qualified
SDK cut and its normal release/import/pin sequence. Do not merge this proposal or
later sdk-rs work outside the overseer's next-cut instruction.

Before activation, the SDK venue owner must resolve V3 account-mode eligibility,
complete position-census scope/envelope, and native private-socket readiness.
The server/account owner must establish one serialized listen-key owner per API
key and coordinate process replacement: creating a new key expires the old one.
Hosts must redact listen keys in URL logs and preserve Unknown/reconcile without
write replay across reconnect or rollback. Keep applicationId caller fields until
all consumers have demonstrably retired their legacy socket dependency.

A redacted authenticated failure proving a route has already been retired is an
immediate live-bug escalation to the overseer, not evidence that the user's key
needs regeneration. This audit performed no live requests or credential access;
authenticated route availability remains unverified. Existing inert TS V3 gates
stay false; runtime-owner exclusions in that scaffold are not the native inventory.
