# Hyperliquid and HyperliquidSpot Rust hosted signing

Owner: codex-zs5e83. Landing branch: origin/feat/rust-rewrite. Research base:
b24911c14d562e257a901f64355d6d75cdee3c2e. Branch:
feat/rust-hyperliquid-hosted-signing. Plan first; no runtime or admission change
in this checkpoint. Source review and auditor 8x5tm1 GO are required before
activation because the slice crosses registry, provider and WASM host boundaries.

## Beta result and scope

Enable explicitly selected Rust Hyperliquid and HyperliquidSpot accounts whose
agent key stays with a browser/hosted provider, through RemoteSigner and an
explicitly supported PayloadSigner contract. Both halves must work: a keyless
engine dispatching a real command to its provider, and a browser Rust signer
answering the hosted request with its own installed key. No TypeScript venue is
constructed, and no signing failure falls back to a local scalar or another
engine. Public data and already-supported local-key accounts keep working.

The beta mixed-Place/cancel correction (#2248) and Spot smoke profile (#2259)
are landed. They are baseline behavior, not new hosted evidence. Prior public
JS/WASM recordings prove public feeds only; no credentialed live smoke or hosted
signing success has been established. GRVT, KuCoin and TxFlow stay parked.

## Current source and concrete gaps

- construction-rules.json has no remoteSigning declaration for either HL name.
  R1 correctly refuses their delegated construction. The default canSign rule
  still expects a key plus secret/callback; changing that rule alone cannot
  install a valid signing identity or make hosted writes work.
- registry/src/lib.rs passes key and secret into Hyperliquid Options. The venue
  retains them, derives its nonce key from the scalar, signs inline in the one
  trading sender and uses the legacy request path. Spot construction also
  derives public mode from scalar presence, and startup checks key/scalar by
  deriving the wallet. These assumptions must be removed in the provider cutover.
- sign.rs owns the actual ordered MessagePack action hash, big-endian u64 nonce,
  optional vault bytes and fixed EIP-712 Agent digest. trading.rs currently emits
  four action variants: order, cancel, updateLeverage and scheduleCancel. Amend
  is native cancel then create. Spot leverage/margin are local settings and
  scheduleCancel remains unsupported on standalone Spot.
- remoteSignerProvider.ts supports Present plus HmacSha256 and rejects nonce
  receipts, PublicAddress, Secp256k1Digest and dependent graphs. Its fixed global
  algorithm list is not a capability declaration for HL. PayloadSignerRequest
  currently admits only hmac-sha256 and stark. No EVM callback shape exists.
- The landed v2 operation channel, native Kraken executor and peer checks from
  #2221 are real reusable foundations. The present OperationCodec receives the
  work graph, which for a digest signature does not contain the original HL
  action. A hash cannot be reversed to reconstruct an allowlisted descriptor.
  Its JS metadata validator also accepts only managed/Session owners, not the
  genuine Setup owner used by address initialization. Neither gap is covered by
  calling the existing channel with a different recipe name.

References: the enabled Binance/Bybit/OKX/Bitget construction rows, the Bitunix
#2262 plan, RUST_HOSTED_SIGNING_ENVELOPE.md, and
RUST_KRAKEN_HOSTED_NONCE_ADAPTER.md. Some older documents describe unlanded or
superseded stages; current source and exact owner pins govern composition.

## One installed signing authority

Keep three public values distinct: actual signing agent, account/user address
used for reads, and optional vault. The actual agent is proved by the installed
provider, never inferred from wallet, applicationId, vault, stableId or key text.
Use the exact installed Primary signing ref and the engine-sealed VerifiedAddress.
No secondary signing or credential fallback is introduced.

m26ps4 owns TWO credential prerequisites: #2223's local ScalarOnly carrier, then
its reviewed hosted sibling install_scalar_only_hosted_primary. The latter is
proposed only for trusted registry hosted construction, ban-scoped to the agreed
file, with family policy ScalarOnly, Required signing and Secp256k1Digest. It
installs genuine credential absence with the provider's own signing ref, checks
cross-slot collisions, and leaves SetupFacts unchanged. It is NOT an injected
credential_absent bit or an ordinary credential minted from options.key.
These APIs are not claimed landed on the research base.

Local and hosted construction converge on the same refs/authority and provider
recipes. Consume scalar material through trusted setup before retaining venue
options; keep only public account/vault/builder values and identity refs there.
An injected or JSON material-only identity does not acquire trusted absence.
Public mode remains claim-free. Invalid private setup is a typed NotSent refusal.

Run the existing engine-owned PublicAddress setup before private activation,
using its real Setup task, frozen bound, generation, cancellation, result AND
resource-join requirements. Seal the exact 20-byte result to this slot, ref,
domain and engine generation. Do not run setup inside a builder Read, invent a
business owner, or copy configured key text into a SignResult. Unsupported peer
setup refuses private activation while public data remains available.

## Dispatch recipe and native financial behavior

Use a single provider-backed trading sender for local and hosted accounts; remove
its eager scalar path in the same cutover. Select the sealed Primary identity
with the existing HYPERLIQUID_AGENT address domain (RawMs1, MAX_SAFE). Preserve the
legacy namespace/key byte continuity, sharing across perps/Spot, vaults, aliases
and existing same-process TxFlow reservations; no stable-id namespace or seed.
An old engine's already-prepared work is not fenced by sharing only its counter:
stop/drain the old account before replacement. The source must pin that handoff.

Each physical call has the reviewed 20s bound, including admission, preparation,
provider wait, Bind and I/O; a serial root is unbounded. Adopt the landed
release-domains feature for mutation FIFO and 334ms actual-release pacing. Reads
and builder checks do not join the mutation FIFO. No positive-gap barrier or
parallel replacement is manufactured. Amend stays inspect/cancel/verify/create
as required by its actual native dependencies; command ownership/journal facts
must remain truthful throughout partial completion.

Freeze the native action fields before preparation. Reserve only at the real
origin dispatcher and keep the exact nonce in SignReceipt as a canonical decimal
string; reject values above HL's MAX_SAFE before numeric wire conversion. The
peer never reserves or changes that nonce. Reprepare uses the shared stale-claim
repair path, fresh work/receipt and cumulative repair budget, with unchanged
financial fields. No automatic write retry follows transport failure or Unknown.

The atomic work is same-ref PublicAddress + Secp256k1Digest over the native
computed Agent digest. Before publishing a body, attest the outbound method,
host, path, headers and body state; validate the complete work/result/receipt,
20-byte address, 65-byte signature, recovery id and low-s; recover against the
origin-computed digest and require equality with the sealed actual agent.
Render r/s/v from that validated result only. Failure sends nothing, changes no
binding and does not latch unrelated signing. No result-bound second address lock.

Keep existing financial/wire controls, local Spot settings, native order/cancel
cardinality checks, mixed accepted IDs, Unknown projection, accepted cancellation
retirement, source-fill amendment checks, builder fields and dead-man ACK rules.
Builder approval remains a user/browser capability; this slice never signs a
wallet-key builder approval, registration, transfer or withdrawal for the server.
Record actual release pacing, conservative refusals and shared journal projection
in CALLERS. The standard is better than TS, not reproducing its races or quirks.

## Closed structured hosted operation

Proposed family descriptor: hyperliquid.l1/1, shared by both aliases but bound to
the selected alias/product, account, network and provider generation. It carries
method/host/path, a closed typed native action, exact decimal nonce and explicit
vault presence. The peer rebuilds the native action in declaration order, then
recomputes MessagePack, nonce/vault suffix and the fixed Agent domain. It compares
the reconstructed digest/receipt to the origin's retained work before signing.
Unknown/duplicate fields, altered action fields, asset/quantity/price/builder,
wrong network/endpoint/vault, noncanonical nonce and receipt disagreement refuse.
No generic JSON reserialization is allowed to silently reorder the action hash.

Initial action allowlist is the four variants the Rust sender actually emits,
not the broader historical TS relay list. Reject transfer/withdraw/vaultTransfer,
approveAgent, builder approval and arbitrary user-signed EIP-712 domains. No blind
digest signing API or caller-chosen peer refs. Mainnet and testnet are distinct;
the peer checks the installed account's network rather than trusting an envelope.

Reuse ONE v2 structured channel and its account-bound capability negotiation,
exact deadline/receipt/slot/role, complete graph validation, native recomputation,
late/duplicate-result fences and bounded replay state. The origin uses a genuine
managed Mutation owner for actions. Existing v1 HMAC/Bitunix behavior is unchanged.
A v1-only peer refuses this capability before a private command can be sent.
The production RemoteSigner transport and browser receiver must actually expose
this channel; standalone peer fixtures alone do not make an account eligible.

Two shared API directions require core review before source:

1. A closed descriptor must be retained beside the exact recipe/preparation and
   forwarded with that real task. A digest-only SignRequest and advisory Context
   cannot supply it. Prefer the existing owned capture/metadata mechanism if it
   can retain this field with the same lifetime; otherwise have fe7lgm review a
   narrowly gated task-bound descriptor seam. No global descriptor lookup, extra
   fake hash operation or independent pending-task map.
2. The same channel needs a typed setup-public-address capability for the real
   Setup owner, with no Mutation/Session role. Preserve its separate setup
   generation and join semantics; never relax business metadata validation to
   accept role-less work. Exact field spelling and execution ownership are core
   gates, not APIs claimed by this plan.

PayloadSigner is an explicit structured HL scheme to be reviewed with the shared
provider owners: the callback receives the same validated action/nonce/vault/
network descriptor and returns the exact raw-signature/public-address contract,
not an HMAC, arbitrary digest or callback-selected URL/body. The agent address
capability must exist at setup too; a signature-only callback is Unsupported,
not silently treated as remote or local signing. Use the same family validator
and recovery checks for both modes. Preserve RemoteSigner precedence, frozen
public-only callback receivers, abort/dispose behavior and sanitized failures.
Do not claim payloadSupported until this exact contract and JS/WASM tests pass.

Construction rows enable both aliases only after each implemented mode passes
its gates. dispatchSigning, algorithm support, setup policy, private-feed policy
and hosted codec declarations must agree in native and SDK generation. HTTP
callback headers cannot supply authentication; private WS reads subscribe using
the actual user/vault and do not fabricate signing work. Decide and test the
payload REST-only policy explicitly against native startup/readiness before
setting payloadSupported. Unsupported modes remain a typed setup refusal.

## Originals and acceptance evidence

Commit compiling controls before changing shared runtime. Distinguish true
assertion REDs from setup/compile failure and entry-gated suffixes.

1. Both aliases: a real keyless engine plus authenticated signing peer obtains
   setup PublicAddress, reaches native readiness, signs a real command, passes
   Prepare/Bind/final Claim and emits exactly one correct HTTP write. A real
   local provider is the working control; no fake Context/identity proof.
2. Native action vectors: perps, Spot, merged Spot, triggers/brackets, cancel,
   leverage/margin, scheduleCancel arm/disarm, mainnet/testnet, vault/no-vault.
   Independent official/ethers digest/signature comparison and exact native body
   bytes, not two copies of our own helper as the oracle.
3. Identity: wrong agent, account, slot, provider generation/network/vault,
   unsupported Setup, malformed address, C8 injection, missing carrier, changed
   address and high-s/recovery/receipt mismatch all refuse before transport;
   a later ordinary valid signed operation still works without an auth latch.
4. Hosted descriptor: tamper every financial field, order of hashed fields,
   method/host/path, action kind, nonce/receipt and requested peer binding. Verify
   complete-batch refusal before any callback and no scalar/credential Present.
5. Nonce custody: same-agent perps/Spot/vault sharing, genuine out-of-order final
   Claims and stale repair, peer reserve count zero, safe-integer boundary codec
   controls labelled separately from genuine reservations. No counter rollback.
6. Async ownership: slow/disconnected/refusing provider, timeout at equality,
   cancellation, disposal, late/duplicate replies and clock revision. Require
   resource join/drain, no stale body publication and no extra HTTP write.
7. Financial regressions: mixed Place [accepted, refused] retains structured ID
   and Unknown; prior accepted call/later refusal retains Unknown; mixed Cancel
   does not restore accepted legs; failed amendment never sends replacement;
   scheduleCancel reconciliation is not an ACK. All existing controls remain.
8. Payload: actual explicit callback contract and setup capability; remote wins
   with both callbacks; no fallback after refusal; headers/foreign result graph
   denied; legacy HMAC/Stark callback contracts still pass unchanged.

Use the browser-compatible JS/WASM EngineHost (hosts/wasm/js/host.mjs), explicit
engine: rust selection, the actual family module and production provider/peer
adapter for both halves. Native-only, fixture-engine and isolated codec tests are
supporting evidence, never beta-host proof. Keep production imports free of TS
venue classes. Re-run HL and HLSpot public tapes/replay and validate sealed WASM
build inputs. Credentialed smoke needs separately supplied mainnet small-balance
agent-wallet keys via oneshot-secret peek; never master keys, logs or test secrets.

## Composition, sizes and delivery

Coordinate shared provider literal/schema/codec changes with Bitunix #2262 and
Kraken's active hosted lane. 9czdjh confirmed no authored shared runtime edits at
73521d53441 when this research began. m26ps4 owns credential carrier source;
fe7lgm reviews the descriptor/setup boundary. Recheck their immutable source pins
and landed APIs before dependent implementation; do not stack on assumed source.

Measure the local composed combined once for shared/provider feature adoption;
CI is the record for HL family sizes. If a core change reaches other families,
measure only the specific pairs required by its owner/ruling (normally Bybit and
Bitget), one cargo build at a time, jobs=4, deleting own target after each. Keep
budgets fixed except explicitly approved standing allowance adjustments, reported
separately. Feature-off/nonadopter checks must demonstrate the actual cost.

Then scoped native and JS/TS tests, strict clippy/fmt/typecheck/lint, bans, hosted
JS/WASM round trips, HL/Spot replay measurement, current-tip composition and one
reviewer CLEAR. Publish source only after the plan/shared directions are settled.
Send exact head/tree, correctness, family/combined and hosted evidence to 8x5tm1;
merge only on auditor GO with a head-matched guarded merge. No CPU wait or
production-bundle release is implied by this beta branch.

Protocol references: [official signing](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/signing)
and [nonces/API wallets](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/nonces-and-api-wallets).
These distinguish L1/user signing and actual-agent nonce sharing; source vectors
and actual JS/WASM producer-to-release controls remain required.
