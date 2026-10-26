# Hyperliquid and HyperliquidSpot Rust hosted signing

Owner: codex-zs5e83. Landing branch: origin/feat/rust-rewrite. Research base:
b24911c14d562e257a901f64355d6d75cdee3c2e. Branch:
feat/rust-hyperliquid-hosted-signing. Plan first; no runtime or admission change
in this checkpoint. Source review and auditor 8x5tm1 GO are required before
activation because the slice crosses registry, provider and WASM host boundaries.

## Post-beta scope and current beta result

Hosted HL/HLSpot accounts are post-beta by the overseer disposition relayed in
fe7lgm's plan review. Beta stays local-key only, and R1 continues to refuse hosted
accounts as NotSent. This lane must not block local-key beta gates or smoke.

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

The shared prerequisites are explicit, separately owned slices; an unpublished
follow-up has no invented PR number or assumed landed API:

| Prerequisite | Owner and source/review boundary |
| --- | --- |
| Local ScalarOnly carrier #2223 | m26ps4; must land before the hosted carrier |
| Trusted hosted signing-only construction, install_scalar_only_hosted_primary | m26ps4; follow-up PR after #2223, not published at this checkpoint; registry/src/hosted_signing.rs is the exact trusted ban-scoped caller |
| Approved descriptor carrier (A), extracted from deferred Paradex mode 2 | m26ps4; standalone shared PR after the hosted carrier, not published at this checkpoint; 9czdjh shared-module review, kuhwxi security review, 8x5tm1 GO; fe7lgm remains a core reviewer |
| Setup-owner admission in shared JS metadata/v2 channel | 9czdjh; follow-up PR not published at this checkpoint; fe7lgm sign-meta/core review |

The hosted construction prerequisite requires family policy ScalarOnly, Required
signing and Secp256k1Digest. It installs genuine credential absence with the
provider-declared per-slot agent signing ref from the trusted provider binding,
never options.key or caller JSON. A collision with ANY other slot's credential
or signing ref refuses. SetupFacts stay unchanged. This is not an injected
credential_absent bit or an ordinary credential minted from a public address.
The sole address proof is the engine's genuine Setup result. An unsupported
Setup capability (including a v1-only peer) refuses private activation with no
partial installed hosted identity. These APIs are not claimed landed here.

This marker is also a provider security boundary. LocalSigner refuses Present
of a declared asymmetric ref everywhere, but the hosted/remote path relies on
the ENGINE's identity-keyed check, which covers a credential-absent primary's
signing ref. Therefore hosted HL MUST use install_scalar_only_hosted_primary;
an ordinary credential-plus-signing identity would evade that specific check.
Pin Present and Credential interpolation of the agent ref as refusals BEFORE
any hosted callback. Do not rely on a remote provider copying LocalSigner policy.

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
vault presence. At the ORIGIN, codec describe reconstructs MessagePack, the
nonce/vault suffix and fixed Agent digest from that descriptor plus its trusted
binding, and attests byte-equality with the ENTIRE actual work graph. The peer
receives only the descriptor, receipt and authenticated binding, not the origin's
work or refs. It independently rebuilds the native action in declaration order
and the exact work with its OWN binding/installed refs before signing. On return,
the origin validates the complete result against its retained original work,
including receipt, address and signature recovery. Neither endpoint claims to
inspect the other's opaque refs or retained work. Unknown/duplicate fields,
altered action fields, asset/quantity/price/builder,
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

codec.replayUntil and a bounded freshness validator are mandatory. The origin
records a genuine prepared stampMs in the receipt, independently of its reserved
nonce. Require a finite 5s signing-freshness window from that stamp, capped by the
original owning deadline; never infer preparation time from the nonce. Native
HL nonce admissibility (T-2days to T+1day) is NOT a signature-task timestamp or a
reasonable tombstone lifetime. Retain the decision until
max(original deadline, prepared stampMs + 5000ms), with the existing bounded
per-account replay capacity. Replaying a used task/receipt with a fresh routing
id or a new deadline after the original deadline still refuses. Refuse timestamp,
receipt and clock-revision disagreement, pin exact equality/late-result controls,
and record the conservative freshness bound in CALLERS. PublicAddress Setup
keeps its separate setup deadline/join lifecycle, with no fabricated business
stamp or nonce.

Consume the approved descriptor carrier (A), owner m26ps4; do not design a new
seam. SignStep::descriptor() -> Option<String> defaults to None, preserving
existing recipes. Only for a hosted provider, the engine copies that descriptor
into sign-meta's optional closed descriptor string (at most 4 KiB); invalid
presence/size refuses. The shared JS metadata validator accepts exactly the
reviewed closed keys plus descriptor. The descriptor grants no owner, role,
slot or binding. HL's codec describe recomputes the native digests from it plus
the trusted binding and requires byte-equality with the ENTIRE original work
graph, not merely its final digest. Preserve task/preparation lifetime, with
no global descriptor lookup, fake hash operation or second pending-task map.

The native setup-meta shape is already landed (#2199):
SignKind::Setup { generation, operation: PublicAddress, slot }, serialized as
owner { kind: setup, generation } plus setup { operation: publicAddress, slot }.
The remaining prerequisite is shared JS validation and v2 channel admission,
owner 9czdjh with fe7lgm review. Admit Setup only for PublicAddress-only work,
with its exact slot, no business role and no credential. Keep the real setup
generation, result AND resource-join contract and RetireAddressSetup lifecycle.
Do not manufacture a Mutation/Session owner or accept arbitrary role-less work.
This source must land before HL consumes it.

PayloadSigner is an explicit structured HL scheme owned by this PR, with
9czdjh shared-host and kuhwxi security review of PayloadSignerRequest. It uses
that SAME v2 frame/channel and codec: the validated action/nonce/vault/network
descriptor in, the family codec's complete SignResult shape and exact receipt
out. No parallel raw-signature/public-address protocol, HMAC reinterpretation,
arbitrary digest or callback-selected URL/body. Setup capability must use the
same channel too; a signature-only callback is Unsupported, not silently treated
as remote or local signing. Use the same family graph validator, result recovery
and replayUntil fences for both modes. Preserve RemoteSigner precedence, frozen
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
   Hosted installation takes only the provider-bound agent ref, rejects every
   cross-slot ref collision and rolls back unsupported Setup without a partial
   identity. A recipe naming Present or Credential interpolation of that agent
   ref refuses in the engine before any callback, on nonce and non-nonce paths.
4. Hosted descriptor: tamper every financial field, order of hashed fields,
   method/host/path, action kind, nonce/receipt and requested peer binding. Verify
   complete-batch refusal before any callback and no scalar/credential Present.
5. Nonce custody: same-agent perps/Spot/vault sharing, genuine out-of-order final
   Claims and stale repair, peer reserve count zero, safe-integer boundary codec
   controls labelled separately from genuine reservations. No counter rollback.
6. Async ownership: slow/disconnected/refusing provider, timeout at equality,
   cancellation, disposal, late/duplicate replies and clock revision. Require
   resource join/drain, no stale body publication and no extra HTTP write.
   Pin genuine prepared-stamp freshness, nonce/stamp independence, bounded
   replayUntil retention and used task/receipt replay with a fresh routing id or
   extended deadline. Neither a two-day nonce window nor 64 evictable tombstones
   may authorize replay after the original task expires.
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

Compose shared provider/codec edits only AFTER Bitunix #2262 lands. Its owner
4yjlw0 supplied source c85ca651b15, head cacb23f8b6a/tree 953a457a3ed, still awaiting
review/GO at this checkpoint. It owns remoteSignerProvider.ts's fused matcher
and per-venue remoteSignerAlgorithms(name), delegatedSignerSetup.ts algorithm
advertisement, hosted_signing.rs OutputKind::layout(), host input format and
legacySigning.ts, plus only the Bitunix construction row. Add HL matcher/output
siblings, retaining that ONE per-venue algorithm switch; do not edit them in
parallel or assume their unpublished APIs have landed. Preserve HMAC behavior.

Coordinate Kraken's active hosted lane with 9czdjh, who confirmed no authored
shared runtime edits at 73521d53441 when this research began. Credential and
carrier sequencing is #2223 -> hosted signing-only construction -> carrier (A),
all owned by m26ps4; the shared Setup-channel slice belongs to 9czdjh. Recheck
exact published PRs, immutable API pins and actual landings before dependent
implementation. This plan's credential direction is CLEAR at 092018dcf37 with
the reviewed corrections folded here; no source approval follows from that.

Measure the local composed combined once for shared/provider feature adoption;
CI is the record for HL family sizes. If a core change reaches other families,
measure only the specific pairs required by its owner/ruling (normally Bybit and
Bitget), one cargo invocation per lane at a time. Use CARGO_BUILD_JOBS=2 for
test/check builds; 4 only for a single WASM/size build. Delete own target after
each measurement/build; an exit 137 is rerun under the machine rule. Keep
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
