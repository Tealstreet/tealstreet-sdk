# GRVT EIP712 provider recipe and installed construction

Owner: codex-zs5e83. Reviewers: codex-9czdjh (native recipe and construction)
and claude-fe7lgm (serial call and auth-resend interplay). PLAN ONLY, following
the overseer's assignment after #2086. No builds, new tests, runtime, constructor,
manifest, feature, host or credential-selection edits at this checkpoint.

This is the recipe/construction prerequisite of RUST_GRVT_WRITE_FUNNEL.md,
whose original-control pin bf61d06542a remains unchanged. Native observations
below are pinned to actual 14b6989e06141ee3d166980c81115267f6d20e85. This
document does not claim that the reviewed shared proposals are installed APIs.

## Minimum material and the hosted model

For an account that can place orders, trusted setup needs exactly two secrets:

| Supplied material | Purpose | Installed representation / use |
| --- | --- | --- |
| GRVT API key, exact native bytes | POST /auth/api_key/login, body field api_key | Primary credential ref; provider Present only in the outbound login recipe |
| GRVT order-signing secp256k1 scalar | EIP712 signature and actual signer address | Primary signing ref; Hex decoding of the existing secret input, exactly 32 bytes and a valid nonzero scalar; declared Secp256k1Digest |

No passphrase, extra wallet, mnemonic, separate L1 secret, nonce secret, read key,
pre-supplied cookie or claimed signer address is required. Public inputs are the
existing testnet flag, order/market metadata and trading subaccount selection.
Native discovery can select the trading subaccount; an explicit
trading_account_id remains needed when native discovery cannot resolve it.
The signer address is derived from the installed scalar. Optional signer_address
keeps its native mismatch warning; it never replaces the derived signer.
Builder authorization and the API key's signer tagging are venue permissions,
not additional signing material to collect. Preserve both native refusals.

Sam's hosted model is the same installed-provider model used for other venues:
the hosted provider may receive these two secrets, and only these secrets, to
log in and place orders. A local provider consumes the same minimum setup on the
client/native host. An external provider may retain the material elsewhere and
serve opaque refs through the same SignRequest operations. A user choosing that
mode is not forced to export the scalar to the hosted service. Do not implement
a GRVT server signer, new secret store or separate hosted credential format.
This mirror plans the Rust/registry contract; any tealstreet-next hosted product
integration is outside this assignment and follows the mirror boundary.

The existing generic operations suffice: Present(credential),
PublicAddress(signing) and Sign(Secp256k1Digest, signing, digest bytes). No new
algorithm, ExtraKey role, legacy SignPayload or browser-only wallet requirement.
The recipe gets refs and public data; raw API key/scalar never enter venue
Options, Context, metadata, logs or Debug. Outbound login presentation is allowed
only in the provider apply result and follows existing secret-copy disposal.

## Installed construction, not a caller-made identity witness

At the pinned actual tree, GRVT still receives Options.key/secret from the
registry and signs eagerly in trading::placement. The current engine provides
set_signer and set_identities before start; registry build_with_signer is the
injection boundary. There is no new InstalledGrvt token API to assume exists.

The proposed direct cutover uses those shared boundaries:

1. Trusted native and JSON/WASM setup consumes the legacy key/secret once.
   Apply primary policy trim_credential=false, secret_encoding=Hex and
   signing_algorithm=Secp256k1Digest. No passphrase or extra ref. Keep native
   acceptance of prefixed/unprefixed scalar input, subject to exact key-format
   vectors; do not broaden accepted values silently. Wipe consumed/rejected
   material on every path using the existing setup/provider guards.
2. The shared provider creates the opaque primary Identity. For external setup,
   resolve the supplied refs and declaration through the existing provider
   contract; a nonempty KeyRef string alone proves neither installation nor
   Secp256k1Digest support. Capability Unknown/Unsupported cannot admit a write.
3. Construct GRVT with non-secret options and retained opaque selection state;
   install that same provider and exactly the same Identities in the Engine
   before start. Partial construction does not start sockets/login or publish
   provisional orders. No guessed ref, fake stable_id, synthetic primary slot,
   key-hash namespace or provider-private material getter is permitted.
4. Select SessionAuth for login presentation and its Primary private egress.
   Select Mutation for order signing and preserve that selection/credential on
   the serial call. GRVT has no secondary slot; do not borrow another venue's
   Secondary fallback or require Kraken's monotonic nonce authority.
5. Before any placement/queue effects, require the actual selected signing ref
   to support Secp256k1Digest, using existing command capability admission.
   PublicAddress must refer to that same declared ref. A recipe cannot switch
   slots or read raw options when a provider is missing, refuses or expires.

Public-only construction must remain keyless. API-key-only private account/login
behavior and the identity endpoint's native trim/nonblank eligibility are
explicit constructor source gates: do not require a scalar merely to log in,
fabricate a signing ref for a credential-only account, or infer key contents
from a ref. The final installed setup must carry the native eligibility facts
through the shared construction model. Exact fit for these partial configurations
is OPEN to the construction review; any missing credential-only shared seam is
owned in credential/registry core, not solved by retaining raw keys in GRVT.

Before source, construction originals must separately pin a missing API key,
an empty supplied key, a whitespace-only key and a nonblank key, each with and
without a scalar and with public-only intent. Native login checks Option
presence while fetch_identity checks trim().is_empty(); one generic ref-presence
test cannot prove both. Record the native startup/login/identity diagnostic and
I/O precedence before changing setup, and obtain m26ps4's shared disposition
for representing these facts without a synthetic signing ref or raw key holder.
Any refused legacy configuration that changes its failure boundary needs an
explicit behavior ruling; this plan does not silently collapse those cases.

Adoption will enable dispatch-signing for the GRVT registry family, declare its
setup policy in the existing shared construction table, and use the existing
native/WASM provider bootstrap. A public address can be fetched through trusted
fetch_public for the native startup warning; placement still validates its own
atomic provider address/signature result. Existing unsupported legacy
remoteSigner/payloadSigner construction rules must be audited explicitly:
provider-backed operation support is not a promise that those older callback
shapes can present a login credential. No silent fallback or invented adapter.

## Login recipe closes the API-key half of the constructor

The constructor cannot strip key from Options while leaving account::login to
serialize it. A small login SignStep is part of the same future cutover:

- Pin POST to the native edge host (mainnet or testnet) and
  /auth/api_key/login. Its unsigned static headers remain Content-Type:
  application/json, X-Grvt-Remember-Me: true, Cookie: rm=true, in that order.
- One provider batch contains only Present(selected primary credential).
  Validate the exact result shape/receipt and build the native JSON body with
  the existing serializer in apply; do not interpolate an API key in venue code.
  This login request is not bound to the cached Session header bundle.
- Retain #1905's real login Session operation, Primary session_egress,
  preparation-failure delegation, login retry budget and failure retirement.
  A failed/expired provider result is a strict unreleased failure, never parsed
  code 1000, a network retry grant or evidence that a private request executed.
- A successful refresh installs the complete native cache and advances its
  checked revision even when returned values are unchanged, then announces
  session_ready. Failed/unproven login retires the real Session. No substitute
  generation or startup-time pretend Ready handle is minted by construction.

This changes the login's credential presentation, not its account validation,
HTTP classification, endpoint, body bytes, Session lifecycle or native timers.
Coordinate account.rs with the login owner before any future source change.

## Immutable placement and EIP712 work

Keep the native pure digest/decimal/schema code in sign.rs. Expose its pure
digest internally for the recipe and remove eager scalar access/signing from
placement when the funnel adopts. No new EIP712 implementation is needed.
Refactor pure validation separately from Step materialization: today's initial
and queued-front planning may generate and discard signatures. A validation
pass must not invoke a provider, mint a wire nonce or cache signed request bytes.

At the real Step materialization, after genuine owner/call reservations and
before a provisional Store effect, freeze once:

- selected subaccount uint64, chain 325/326, immutable leg asset ID, exact
  integer contract size/price, side and market/TIF/post-only/reduce-only flags;
- native main/test builder address and fee 0.03 (signed uint32 300), including
  Order versus OrderWithBuilderFee schema discrimination in pure vectors;
- UUID(signing entropy) -> Keccak -> first four bytes as big-endian uint32
  nonce, one-hour expiration in nanoseconds, creation time, client IDs, trigger
  metadata and original JSON field/omission order. Client-ID generation uses
  identity entropy, with the native collision guard and explicit-ID rules.

These fields live in one immutable placement payload, shared with its serial
call. No monotonic reserve_nonce/NonceClaim applies to this random nonce.
SignStep::prepare only reads frozen fields on every Stale and auth resend.
The protocol clock/revision may fence work; it cannot renew the payload's nonce,
expiration, creation time or client IDs. Bracket/replacement Steps get their own
native materialization, never borrow another order's signature or digest.

The placement SignStep accepts only native POST /lite/v1/create_order at the
native trades host. Its unsigned header is Content-Type: application/json.
prepare builds one atomic SignRequest with PublicAddress(selected signing ref)
and Sign(Secp256k1Digest, the SAME ref, the exact 32-byte EIP712 digest).
Domain is GRVT Exchange, version 0, chain 325/326; no personal-sign prefix,
double hashing, float ABI units or provider-built order semantics.

apply verifies result count, exact receipt and raw lengths (20-byte address,
65-byte r||s||recovery 0/1), rejects invalid/foreign signatures with existing
ethereum::signed_by over that exact digest/address, and serializes the existing
Create payload. Preserve native checksum signer text, 0x-prefixed 32-byte r/s
and wire v=27+recovery, decimal expiration/nonce and chain text. The native
sign_order/oracle vectors settle this conversion; do not forward raw 0/1 as v.
Malformed, wrong-ref/receipt, high-s or wrong-digest results never reach Bind.

The SignReceipt carries the fixed payload expiry in protocol milliseconds,
not a newly extended expiry or inferred timestamp. Shared dispatch also keeps
the ruled 20 s current-call window. Reprepare cannot release an already expired
order even if the serial root is live. Exact receipt and freshness integration
is reviewed against the actual feature pins before source, without a timer or
clock policy hidden in the recipe.

The native local signer is deterministic. Native auth resend preserves the
entire signed Step.body, so fresh provider work for the same digest/key must
reproduce that body, not just a semantically equivalent signature. Original
vectors must prove this with the normal provider and the supported external
fixture. The generic algorithm's structural length check alone does not prove
determinism: any provider compatibility gap is an explicit review/behavior gate,
not permission to cache/replay signed bytes or silently change a client order.

## Session headers, unsigned calls and same-call resend

The placement/login recipes emit none of the private bundle's three names.
For native trade requests #2069 supplies X-Ts-Grvt-Session, Cookie with exact
value "rm=true; gravity=<cookie>", then X-Grvt-Account-Id. GRVT declares
SessionPlacement::After("Content-Type") with the bundle; shared assembly keeps
native Content-Type first, checks collisions and combined limits, and wipes
secret copies. The immutable order payload never contains cookie/account headers.

Cancel, leverage, Inspect, Verify and ReadSymbol require no EIP712 signature.
They still use managed Session-bound HTTP with the native body, Primary egress
and call operation. An unsigned Read does not need a fake signing credential;
it retains the same Session revision fences and parsed-1000 auth ledger.

For parsed code 1000 DATA, including 401/408/503, the funnel's narrow
status_exempt_refusal hook and classify_write proof authorize the ONE resend.
Every SerialCall reserves attempts=2. serial_plan_session_resend returns
attempt 2 of that same call, seeded with the refused bundle revision. Rebuild
fresh provider work from the SAME immutable placement with the SAME selection;
no new Step, nonce, expiry, client ID, logical Mutation or network-budget reset.
Queue that Context bound to the real Session. #2069 holds it until Ready with a
newer revision. session_ready from login completion wakes it within its window.
The recipe does not start login, spend another grant or implement a local wait.

Refresh between Prepare/Bind and Claim also uses #2069's unreleased park,
including unsigned Reads; it spends no auth resend or attempt. Ordinary Stale
from route, signing freshness, clock revision or host reprepare must still bind
at an unchanged Session revision. Only the moved/cleared-bundle claim fence or
the resend seed raises the minimum revision. Retirement/expiry goes through
the shared terminal projection with prior released facts dominating NotSent.

GRVT issues one live call at a time in its FIFO/serial lifecycle. Multi-order
commands remain sequences; this consumer needs no Session-bound Mutation group
or R11 group-claim revision check. #2069's issue-time group refusal is compatible.

## Feature opt-in and adoption gates

All three pending engine features are non-default. When GRVT adopts the funnel,
its safe-cex-engine dependency will explicitly enable **serial-plans** (#2092),
**condition-waits** (#2094) and **session-bearer** (#2069), along with existing
required features. Do not enable these globally or for unrelated venue families.
The GRVT registry family also opts into shared dispatch-signing; local crypto
uses the existing secp256k1 feature. Exact Cargo spellings/transitive edges and
refrozen source pins are rechecked against the owner branches at adoption.

| Gate | Owner / review needed |
| --- | --- |
| Serial calls, condition waits, immediate Read continuation and seeded auth resend | fe7lgm; #2092/#2094 and the actual resend source, including status-exempt parsed 1000 |
| Bundle placement, floor-preserving park/reprepare and wake after refresh | m26ps4; #2069 actual feature source and its revised floor tests |
| EIP712/Create and login recipes, trusted setup, installed registry/native/WASM path | zs5e83; 9czdjh native/construction review and fe7lgm resend fit |
| Existing login lifecycle and callback composition | zs5e83 with 55ao7t's login contract; exact real login preparation-failure control retained |
| API-key-only and identity eligibility without raw key retention | Shared construction/credential owner proposal m26ps4, consumer zs5e83; exact existing seam or named shared gap before source |
| External provider deterministic same-body resend | 9czdjh/fe7lgm confirm exact primitive/provider contract; no generic API widening assumed |

## Later originals and preservation proof

These are requested next-phase tests, not compiled controls or claimed REDs:

1. Native order and builder EIP712 vectors for both chains, exact wire body and
   checksum signer; 65-byte provider parity conversion and signature recovery.
2. Installed local versus async external provider produces identical placement
   bytes through real Prepare/Sign/Bind/Claim. Capture SignRequest: exactly the
   two order operations, no API key/scalar or extra material in venue state.
3. Provider login Present produces the exact native edge POST/header/body;
   no scalar/payload operation, no private bundle on login. Missing provider,
   undeclared/wrong algorithm, bad scalar, wrong receipt/result/signature and
   delayed provider expiry each fail once, with no raw fallback or provisional
   order. Include public-only and API-key-only constructor controls.
4. Parsed-1000 single resend through actual serial DATA custody preserves the
   full order body and two independent auth/network ledgers. Second 1000 has
   no third send. Cover 401/408/503 x parsed/nonparsed bodies and shared outcome
   disclosures already pinned by bf61; no 408/5xx-wide resend permission.
5. Stale reprepare, account-ID-only rotation and identical-value refresh keep
   nonce/expiry/client IDs/digest fixed. Mutation AND Read refreshed between
   Prepare and Claim park the same call, reissue once after a newer revision,
   and remain strict on root/call expiry or retire. Ordinary same-revision
   Stale remains a working control. Assert the final released packet, not just
   Prepare.request, and prove no duplicate provisional Store projection.
6. Recipe-header exclusion, Content-Type-first order, combined limits, redacted
   failures and late callbacks; installed constructor refuses caller-forged
   refs/capability claims and post-start identity/provider replacement.

Retain bf61's original target unchanged, native trading/signing financial and
wire/oracle assertions, callback/login controls, and all unrelated incoming
source/maps. This recipe plan removes **zero inventory sites**. Future five-site
write adoption alone owns that prune; no renamed-reference or isolated-terminal
claim. Document the existing ruled 20 s bounds and journal differences in CALLERS
when source adopts; this plan adds no new timeout, retry or metadata ruling.

No Cargo, tests, WASM, size builds or CI were run for this plan-only checkpoint.
Source remains gated on reviewed originals, exact installed-constructor fit,
shared source landings/refreeze and a fresh production scope instruction.

## Bounded native plan review

9czdjh's read-only review at ca7d5c67782 is CLEAR for the stated native/recipe
direction: schema, domain, integer units, entropy, login bytes/header order,
atomic same-ref operations, parity conversion and immutable Step requirements.
No GRVT build was run for that review. Credential-only installed construction
and deterministic external-provider full-body resend both remain OPEN. This
receipt is neither installed-constructor nor production/source clearance;
LocalSigner alone cannot discharge the external-provider gate.
