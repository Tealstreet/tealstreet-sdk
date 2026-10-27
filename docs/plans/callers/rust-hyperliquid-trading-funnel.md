# Hyperliquid local trading custody

Applies to Hyperliquid and HyperliquidSpot SDK, browser/WASM, worker and native
callers when the complete local trading cutover lands. A recipe checkpoint by
itself does not activate the venue. Hosted remoteSigner and payloadSigner remain
unsupported; this change adds no hosted path or TypeScript venue fallback.

Trusted setup consumes the agent scalar before constructing the venue. Generic
options retain only public account configuration and installed references. The
key, applicationId and vault options are public routing/account values, never a
presentable credential or a nonce subject. Callers must provision the local
provider in the executing host and complete engine-owned public-address setup
before start. Unsupported or failed setup refuses private work. Copying retained
exchange options cannot provision another signer.

Scalar text remains byte-exact: surrounding ASCII space, NEL and BOM refuse.
Hex decoding accepts an optional 0x/0X prefix and hex letter case; the provider
uses the same decoder on native and SDK/WASM paths. The old SDK Wallet's 0X
rejection is superseded by that shared native decoder. Invalid input produces
fixed errors without the scalar. Spot validates its configured agent against the
engine-proved address; perps keep the configured-agent mismatch warning.

Every write waits for real Prepare, provider completion, Bind and final Claim.
The command root is serial and unbounded; each physical call has an unchanged
20-second bound that preparation or pacing cannot renew. A 334ms mutation-only
FIFO gap starts at actual release rather than eager queueing. Reads do not join
that FIFO. Builder readiness keeps its existing eight-second limit. Callers must
not treat a preparation or a provider result as evidence that a write was sent.

Unknown is never retried as a write. The existing fresh-order reconciliation
fence stays active until a complete authoritative read releases the relevant
scope. Structured accepted IDs survive mixed batches and later failures. Native
order values, bracket grouping, cancellation proofs and amend dependencies stay
pinned. Shared write-journal terminals replace numeric-ID completion authority.

The actual proved agent keys the existing Hyperliquid/TxFlow nonce namespace,
across engines, networks and vaults in one process/WASM memory. A new engine must
not inherit live legacy host work: cancel or fence its unsent release, join its
actual work/cleanup, then initialize the new engine. Disposal bookkeeping or a
shared reserved maximum alone does not prove retirement. Separate processes or
WASM memories retain their existing deployment coordination obligation.

This clears the local venue raw-scalar holder and eager HTTP/signing callers
only with the full cutover and its production-binding controls. Standalone
native signing/vector helpers still consumed by the disabled TxFlow family are
not an HL runtime fallback. Their separate deletion requires that consumer to
migrate or disappear. order_read_epoch adoption follows in a separate slice.
