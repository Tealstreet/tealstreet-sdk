# Legacy nonce-domain handoff — approved shared prerequisite

Base: SDK master 97cfa6d90a1; branch feat/rust-nonce-domain-handoff. Plan GO: claude-8x5tm1, engine-only scope; no activation. One PR into master; first consumer/reviewer codex-9czdjh (#2302).

1. Trusted association and exact subject. Add an opaque LegacyNonceAssociation, issued by the Engine for its current installed identity, exact credential/signing refs, stable subject, domain and a private generation/issuer seal. Obtain the legacy SHA256 key through an unconditional default-refusing Signer hook, called only by trusted initialization, never through Cx or business signing. A private engine-minted request fixes the installed ref; the trusted key owner answers from those same decoded signing bytes. No public constructor taking a digest plus subject, no secret-derived stable_id, serde or venue-text association. A source digest cannot bind two targets; multiple proven digests may bind one subject in one transition. Reinstallation/current-identity checks invalidate stale authority. Only StableId domains qualify.

2. Owned issuance and join. Add LegacyNonceIssuer, LegacyNoncePermit and a retained LegacyNonceWork future wrapper. Registration, permit acquisition and reservation use the existing NONCES mutex. Every owned prepared/queued/provider/transport operation retains its permit in the original work+cleanup future. The wrapper retires its private work record only when that original future is Ready, including settled errors. There is no public joined(bool), raw completion token or drop-as-join. Dropped/cancelled/unpolled owners remain outstanding and cannot authorize replacement. Hosts retain/poll the original wrapper independently of Engine lifetime. No user callback is run under the nonce lock.

3. Fence and atomic takeover. A non-Clone/non-serializable NonceTakeover capability closes new source registration/reservation and target claim admission for the complete associated source set, under that same mutex. Existing permits may finish and report physical release, then join. Commit requires the exact fenced generation, current identity/association and zero outstanding owned work. In one locked transaction, import the maximum reserved and maximum released values across ALL sources AND the existing target; conservatively include every tracked legacy reservation in the released floor only after its original owner joins. Historically untracked work refuses takeover. Preserve exhaustion, refuse partial imports, and permanently fence the retired legacy entries. No caller-supplied maxima. Raw old entry calls cannot silently supply owned retirement evidence. Their coverage gap, other processes/WASM memories and external issuers are explicit CALLERS boundaries; this PR does not activate a host rollout.

4. Genuine claim flow. After commit, existing select_with_nonce -> NonceClaim -> reservation -> Prepare/sign/Bind -> final Claim is the sole target path. Validate the association epoch at mint, reserve, identity/current checks and final release so pre-fence claims cannot cross the transition. Concurrent source reserve/release, target reserve/release, fence and import serialize on NONCES; unrelated namespaces continue. No alias fallback or second allocator.

5. Invalid-nonce refusal. Add a distinct classifier verdict for definitive nonce refusal, producing NotSent and NEVER RefusedResend. Existing transport/408/5xx uncertainty precedence and earlier root effects/Unknown remain intact. The consumer supplies its exact documented native-code row in its adoption PR; this shared PR contains no Kraken/venue table and authorizes no mutation replay.

Files: engine/src/nonce.rs and new nonce_handoff.rs/tests; nonce_claim.rs; signing.rs (inert default provider hook/types); engine.rs/trusted initializer; classify.rs/classify_tests.rs and affected exhaustive matches; genuine managed-dispatch production-binding controls; RUST_CUTOVER_CALLERS.md plus short handoff plan. Runtime ownership, association, fence and takeover machinery sits behind the default-off `nonce-handoff` engine feature, which implies `dispatch-nonce`. No venue or host enables it in this PR. Public trait additions remain unconditional defaults appended after existing hooks.

Scope: the LocalSigner/crypto implementation is EXCLUDED. This PR adds only the default-refusing provider contract. The real decoded-key-store hook requires its own signing/credential plan and GO before Kraken activation; native/WASM/worker ownership forwarding also remains an adoption gate. No venue tables or host rollout ship here.

Additional GO conditions: commit classifier originals before its source change; cover transport/408/5xx and earlier Unknown precedence. NotSent follows the existing terminal path and cannot clear another uncertain placement or certify a stale census. Every failed takeover preserves both floors and the entire fence phase. CALLERS changes append boundaries only. Report non-adopter family byte deltas, including the ungated classifier/default-hook surface; use CI family measurements and one combined measurement. First-consumer CLEAR from 9czdjh and exact-tree auditor GO on green correctness remain required.

Verification: held actual owner blocks takeover despite response/timeout/drop; successful real join permits exactly one commit; concurrent reserve/release imports both maxima and higher target floors; two source digests; forged/mismatched/stale/foreign association and stale claim refusal; counter overflow and failed transaction leaves floors unchanged; real managed Prepare/Bind/Claim after takeover uses a nonce strictly above every imported reservation; definitive invalid-nonce response -> NotSent/no resend, uncertain statuses stay Unknown. Scoped engine checks on/off and feature-unified downstream hook implementation; strict clippy/fmt/bans, one combined measurement and CI correctness/family records. CARGO_BUILD_JOBS=2, one cargo at a time.

Execution after plan GO: implement shared types/store and discriminating controls, then managed claim/classifier integration; qualify and self-audit; send exact PR to 9czdjh plus 8x5tm1. No activation claim from engine-only tests.

## Source ownership pins

Every outstanding target-domain owner joins too, including already released work. A target maximum cannot substitute for its original physical-resource retirement. Pre-fence prepared claims are epoch-refused before another release. Registration must precede owned work: historically raw, untracked reservations with no retained original generation refuse takeover; neither an empty new lease table nor a new generation proves old work quiescent. Raw legacy entry calls refuse once that namespace is fenced. This engine-only slice does not create retrospective drain evidence.


## Implemented API flow (source review pending)

`Engine::associate_legacy_nonce(slot, domain, algorithm)` asks the installed
provider's default-refusing `legacy_nonce_association` hook for its exact decoded
key bytes' digest. The request is engine-minted and the proof is request-bound;
neither proof nor association exposes that digest. Only stable-subject domains
are eligible. Providers must validate their own ref and algorithm before calling
`LegacyNonceRequest::bind_key_bytes` on the borrowed installed key material.

Register each old generation before work with `register_legacy_nonce_owner`
(source) or `register_nonce_owner` (target). Move the returned `NonceOwner` into
`run`, passing a factory for the original generation future that joins all of
its prepared, provider and transport work plus cleanup. The wrapper drops the
settled original future before acknowledging its owner. Future errors may retire
resources; dropping or abandoning a Pending/unpolled wrapper never does. A
source's `LegacyNonceIssuer::reserve` produces an opaque `LegacyNoncePermit`;
its release method records only a high-water fact. It grants no order authority.

The replacement Engine takes fresh provider associations, passes the complete
source set to `fence_nonce_handoff`, waits for ALL source and target wrappers,
then calls `commit_nonce_handoff`. Registration, reserve, release, fencing and
import use the one nonce-store mutex. Raw old entry calls cannot pass a fenced
namespace. The target's next owner is registered only AFTER commit, before new
work. Its venue rebuilds from `Cx::nonce_identities` at start; construction
clones never acquire the new owner capability. Existing managed admission,
Prepare and final Claim recheck current identity, owner and epoch. Old Binds
cannot be released after the fence, even if a cached signature was valid.
A late original provider result also terminates without Bind or reprepare when
its epoch is revoked; ordinary nonce supersession keeps its existing repair.

`Verdict::NonceRefused` is ungated but has no venue adopter in this PR. It needs
one exact native code, cannot be a status/unlisted/range refusal, produces
InvalidInput/NotSent and grants no resend. Its effects-free result follows the
existing command terminal path. Earlier Unknown wins, and this terminal never
reports an order census. A census begun before a later uncertain mark remains
unable to clear that fence. Known accepted prefixes retain the existing partial
outcome. Serial refusal summaries retain whether an actual Rejected occurred.


## Auditor-directed dormant feature boundary

The initial dispatch-nonce-gated source measured CI Kraken1,412,211 bytes against
its1,410,048 cap. The proposed3KiB raise is withdrawn: it exceeds the shared2KiB
pre-approval and5% of the1,343,488 base. At8x5tm1's direction, `nonce-handoff`
now gates tracking/owner/epoch storage, the trusted initializer and all handoff
runtime. Ordinary dispatch-nonce keeps its previous reservation and address-only
identity checks; only a reviewed activation will opt Kraken into owned takeover.
Provider hook/types stay unconditional and default-refusing. The classifier
verdict and common NotSent projection remain ungated, with no venue adopter.

CI feature-mode qualification explicitly exercises owned takeover both without
order-locks and with order-locks/serial-plans. Existing family feature closures
remain controls for the dormant state. Remeasure only the ruling-required Kraken
slice locally, use CI for the full family record, and retain the already measured
combined result at its actual source pin. Any final cap is set only from measured
bytes rounded to KiB and gets the required auditor disposition.
