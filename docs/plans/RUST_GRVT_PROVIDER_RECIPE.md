# GRVT EIP712 provider recipe and installed construction

Owner: codex-zs5e83. Reviewers: codex-9czdjh (native recipe and construction)
and claude-fe7lgm (serial call and auth-resend interplay). The original plan-only
checkpoint follows the overseer's assignment after #2086. The local recipe
checkpoint below follows the later explicit source assignment; installed
construction and native command cutover remain held at G3/G6.

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

m26ps4 confirmed this is a missing shared seam: key-only setup currently uses
an empty signing-ref sentinel that Identity::validate rejects, and refs lose
the native credential-presence distinctions. He owns the proposed
RUST_CREDENTIAL_ONLY_SETUP.md plan: optional signing material represented as
explicit absence, plus trusted Absent/Empty/Blank/Nonblank credential facts.
That proposal is not yet an installed API or a complete constructor CLEAR.
m26ps4 accepted the consumer corrections: selection itself never refuses for
absent signing material; only an operation needing a signature requires its ref.
In particular, do not reject every Mutation solely for missing signing material:
native CancelOrders, CancelSymbolOrders and SetLeverage use Session-authenticated
unsigned calls without the scalar. Placement needs the Secp256k1Digest capability;
unsigned native effects do not acquire that requirement. Presence Empty installs
no credential ref: its known non-secret api_key "" is rendered directly, with
no Present or retained material. Missing remains distinct and does not become
an empty login. Blank/Nonblank facts preserve identity eligibility. The shared
plan and exact constructor proof, including native egress, remain required.

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
- For a nonempty supplied key, one provider batch contains only
  Present(selected primary credential).
  Validate the exact result shape/receipt and build the native JSON body with
  the existing serializer in apply; do not interpolate an API key in venue code.
  This login request is not bound to the cached Session header bundle.
- For trusted presence Empty, render the known non-secret api_key "" with no
  Present and no invented ref. Absent does not issue login; Blank remains an
  exact-byte credential for login while identity rejects a whitespace-only key.
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
Bind the expected actual signer to the installed signing ref using trusted
public derivation. Each PublicAddress result must equal that pinned signer
before signed_by verifies the prepared digest. It cannot move with a later
provider answer, native account ID or optional configured signer_address;
the latter retains its warning-only behavior. A different recovered signer or
digest refuses the resend before release, even if its signature is otherwise valid.

The SignReceipt carries the fixed payload expiry in protocol milliseconds,
not a newly extended expiry or inferred timestamp. Shared dispatch also keeps
the ruled 20 s current-call window. Reprepare cannot release an already expired
order even if the serial root is live. Exact receipt and freshness integration
is reviewed against the actual feature pins before source, without a timer or
clock policy hidden in the recipe.

The overseer approved this external criterion: the native local provider keeps
complete wire-byte parity (RFC6979, low-s), including on auth resend. An external
provider may return different r/s/v ONLY: the prepared digest, expected actual
signer and every frozen order/unsigned field remain identical, and the signature
must verify and be low-s. Deterministic external signatures are not required.
Unchanged order identity gives deduplication safety; signed-byte identity is not
its authority. Native body-identical originals remain unchanged local evidence.
Test a real external fixture with two different valid signatures over the same
digest, and refuse wrong-signer or wrong-digest variants. Never cache signed
bytes, renew nonce/expiry/client IDs or reset an attempt to achieve this result.

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

At the resumed actual tip 76f1f7fc842, all three non-default engine features
exist: serial-plans (#2092, ff219538b302), condition-waits (#2094,
ba6fb1912399) and session-bearer (#2069, ad0b74afdf84). This supersedes the
earlier proposal-only status at review. Their presence is not GRVT adoption:
its dependency will explicitly enable them after exact consumer composition
checks. No global or unrelated-family opt-in. The GRVT registry family also
opts into shared dispatch-signing; local crypto uses existing secp256k1.
Seeded serial auth resend and the parsed-status exemption are not present in
this actual tree and remain separate fe7lgm source gates. His CLEARed plan is
6208560a514; REDs and source are not written yet. The status_exempt_refusal
hook is consulted only for Unproven status, and GRVT returns true only for
a parsed full JSON envelope with code == 1000, never a partial/text match.

condition-waits is required specifically for a woken front command that must
re-park from on_http_preflight_ready when the native builder/prerequisite is
still unready: park_preflight_for(preflight, None) retains that command's ready
custody without issuing or failing. Initial entry uses ordinary park_preflight,
and native callbacks use request_preflight_ready; those alone are ungated.
The later control ready_front_reparks_without_issue_when_builder_is_unready
must prove the ready-scope re-park and eventual wake. No bounded wait/timer is
added to the native builder check. If final adoption eliminates this re-park,
drop condition-waits rather than paying for an unused feature.

| Gate | Owner / review needed |
| --- | --- |
| Serial calls, condition waits, immediate Read continuation and seeded auth resend | fe7lgm; #2092/#2094 landed, exact composition and remaining resend source/status-exempt parsed 1000 still required |
| Bundle placement, floor-preserving park/reprepare and wake after refresh | m26ps4; #2069 landed ad0b74afdf84; GRVT ingress/floor/resend composition proof still required |
| EIP712/Create and login recipes, trusted setup, installed registry/native/WASM path | zs5e83; 9czdjh native/construction review and fe7lgm resend fit |
| Existing login lifecycle and callback composition | zs5e83 with 55ao7t's login contract; exact real login preparation-failure control retained |
| API-key-only and identity eligibility without raw key retention | m26ps4 confirmed shared gap; proposed credential-only setup plan, consumer zs5e83. Explicit absent signing plus trusted presence facts; preserve unsigned Mutation eligibility and define empty-key login before source. |
| External provider signature criterion on resend | Overseer ruling CLEAR: local complete byte parity; external r/s/v may vary only with identical digest, pinned actual signer and frozen fields, verified low-s. Actual provider/ingress controls still required. |
| GRVT adoption WASM budget and size review | Adopter zs5e83 and overseer: review the actual GRVT build with serial-plans, needed condition-waits, session-bearer, dispatch-signing and secp256k1 enabled; measured release/CI size and unchanged replay/budget gates before adoption. No assumed headroom or budget increase. No builds at this plan checkpoint. |

## Later originals and preservation proof

These are requested next-phase tests, not compiled controls or claimed REDs:

1. Native order and builder EIP712 vectors for both chains, exact wire body and
   checksum signer; 65-byte provider parity conversion and signature recovery.
2. Installed local provider preserves native placement bytes through real
   Prepare/Sign/Bind/Claim. An async external fixture must prove the same
   digest/signer/frozen fields and verified low-s signature. Pin two different
   valid r/s/v results over the same digest as accepted, a result recovering to
   a different signer as refused, and a wrong-digest result as refused. Capture
   SignRequest: exactly two order operations, no API key/scalar in venue state.
3. Provider login Present produces the exact native edge POST/header/body;
   no scalar/payload operation, no private bundle on login. Missing provider,
   undeclared/wrong algorithm, bad scalar, wrong receipt/result/signature and
   delayed provider expiry each fail once, with no raw fallback or provisional
   order. Include public-only and API-key-only constructor controls.
4. Parsed-1000 single resend through actual serial DATA custody preserves the
   full local-provider order body and two independent auth/network ledgers;
   external signatures follow the approved r/s/v-only variation. Second 1000 has
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
No GRVT build was run for that review. That direction-only receipt did not
clear credential-only installed construction or external-provider fit.

fe7lgm's correction review at a9694bbb008 is CLEAR for plan fit, read-only:
feature proposal status, adoption size gate, the concrete ready-scope re-park,
and ownership of credential-only unsigned-Mutation eligibility are correct.
The subsequent overseer ruling closes the external signature criterion as
specified above; CALLERS records the approved future contract. m26ps4 owns the
shared credential-only plan and accepted the two consumer corrections. Its
actual setup API/source and all recipe/provider controls remain unimplemented
gates. No receipt here is installed-constructor or production/source clearance.

## Resumed credential-only consumer answers: G2-G6

Plain-merged actual 76f1f7fc842 into c13acb15e56, without history rewriting.
Only README and CALLERS conflicted; both retain entire incoming files followed
by the unchanged own suffix. No runtime, feature, Cargo or test change was
introduced. The landed #2108 plan at 9f1d6bbb3275 lists G2-G5; its owner branch
addition aa1f5437a18 supplies G6. The following answers were sent to m26ps4
before GRVT source, and he accepted them as plan fit, not source clearance.

- **G2, placement admission:** require the selected signing ref and declared
  Secp256k1Digest capability in genuine command/Preflight admission before
  queue, builder work or provisional effects. Replacement must prove signing
  availability before its preceding cancellation. Keep native validation order
  explicit; any changed diagnostic precedence is disclosed. Only operations
  needing a signature have this requirement; key-only cancels/cancel-symbol/
  leverage remain available. Later prepare checks remain necessary, and G2
  alone cannot establish the latch behavior in G6.
- **G3, Empty login:** a trusted SetupFacts-derived primary login intent can
  be selected with SessionAuth using generic Slots over non-secret material.
  Empty yields that intent, no Identity/KeyRef/Present, native api_key "",
  credential=None and Primary private egress on the real login Session.
  Absent yields no login intent. Blank/Nonblank use the actual installed
  credential-only primary and Present its exact value. m26ps4 owns the named
  shared fact-to-intent seam; D3 must not make a credential-absent Identity
  valid. This is proposed shared source, not an installed constructor API.
  Pin exact wire and final Bind/Claim egress for Empty with/without scalar;
  no guessed refs, invented provider work or caller-supplied facts.
- **G4, Blank keys:** public-only returns before login. Existing pending,
  authentication-failed and retry gates precede the key Option check. Absent
  emits config.invalid with "GRVT private account requires an API key" and
  no login; Empty/Blank serialize their exact value and do issue login on
  Primary. Identity independently refuses public-only/Absent/Empty/Blank
  before request with "GRVT account identity requires credentials" through
  its native command_error/fail chain. Classify with Rust str::trim before
  setup policy, including NEL-versus-BOM cases; no scalar manufactures key
  eligibility and no key trim changes login bytes.
- **G5, callback boundary:** no existing GRVT legacy remoteSigner,
  payloadSigner or delegated callback needs credential-only support. GRVT is
  outside registry DELEGATED; retain private payloadSigner refusal and the
  existing complete-setup boundary for remote callbacks. Hosted LocalSetup
  with a key but no scalar is the intended credential-only native/JSON path;
  distinguish that trusted setup from external Identities JSON. Future
  complete-ref external order providers remain governed by the recipe's
  reviewed signature criterion, not a key-only callback bypass.
- **G6, Optional-family latch:** an intentionally absent Optional signing ref
  is a per-operation limitation, not an account-wide missing credential.
  The overseer approved the operation-scoped distinction: the operation
  refuses with a typed NotSent failure; the command terminal follows the
  Plan journal. Earlier settled effects cannot be relabeled NotSent by this
  later refusal. It does not poison later login Present/refresh,
  unsigned Session work, secondary operations or startup. Admission normally
  refuses placement earlier, but a late absence must also be safe. Required
  families and an actual MissingKey for a ref declared installed keep #2109's
  latch, even for an Optional family. Do not suppress all MissingKey under
  Optional, hide it as a generic error, add a GRVT exception or restore socket
  reconnect loops. A failing socket signing operation still stops.

G6 controls deliberately reach absent-ref prepare past G2, then prove real
login Present and unsigned cancel/leverage still succeed, with no account
latch/StartupFailed caused by intentional Optional absence. Keep Required-
family and declared-installed-ref missing-key latch controls as positives.
Add a serial case: an unsigned call settles, then an absent-ref placement
refuses at preparation. Pin both the operation's NotSent and the command's
journal terminal: earlier Accepted effects yield outcome None, definitive
refusal history yields Rejected under the common projection. Neither becomes
a whole-command NotSent merely because the final operation was unissued.
Assert no account latch and retain a working subsequent unsigned operation.
m26ps4 records the approved G6 disposition in revision 4 of
RUST_CREDENTIAL_ONLY_SETUP.md after phase-2/D1 lands; kuhwxi reviews its source.
The behavior ruling is closed, while shared plan/source and consumer controls
still gate GRVT adoption. G2-G6 answers do not bypass those gates. Any generic
engine PR measures Bybit and Bitget on its composed tree. No build, source
authorization or push is inferred from resuming this plan.

fe7lgm's exact 4e6c4f4f3bd re-review is plan-level CLEAR for custody and resend
fit. The operation-versus-command precision above folds his follow-up; this
remains a plan and future control specification, not source or constructor
certification. The original bf61 controls are not changed by this wording.

## Resume audit on 2026-10-26

Plain-merged landing tip 34999f973f9 before further work. Only README and
CALLERS conflicted; both keep complete incoming bytes plus the own suffix
once. All incoming runtime, features, inventories and fixtures are retained,
including #2138's GRVT entry-refusal cutover to cx.refuse_command. Do not
restore the earlier temporary admission contexts when the recipe adopts.
The original write-funnel target remains byte-exact to bf61d06542a; none of
its historical RED or later source-gate suffixes was rerun in this docs audit.

Rechecked live prerequisites rather than treating the old handoff as current:

| Prerequisite | Verified state at this audit | Remaining action |
| --- | --- | --- |
| Credential-only phase 2, D1/D2 | PR #2132 OPEN, head 009e5cb3bb20; not in the landing tip | Actual landing before constructor integration; phase 2 alone does not implement the G3/G6 contract |
| Credential-only revision 4, presence facts, Empty login intent and Optional operation-scoped absence | Owner m26ps4 accepted G2-G6 and the serial journal clarification; these consumer seams are still unlanded | Owner revision/source, kuhwxi review and actual adoption proof |
| Seeded serial Session resend and status_exempt_refusal | Owner branch still at CLEARed plan 6208560a514; neither API is in the landing tip | fe7lgm's originals/source and actual landing before GRVT auth-resend adoption |
| Non-default serial-plans, condition-waits, session-bearer | Landed declarations remain available | Explicit GRVT opt-ins and its own composed-tree size/budget review at adoption |

The recipe remains blocked on those constructor and resend source seams;
there is no GRVT source or runtime-clearance claim. This resume does not
dispatch CI or push a partially integrated implementation. Generic engine
changes require Bybit and Bitget measurements on the composed landing tree;
each scratch measurement/review build removes its own target directory after
preserving logs and size tables. No build or measurement ran in this audit.


## Local recipe checkpoint above #2146

The overseer authorized local source above the frozen #2146 composition, with
no push until #2146 lands and no implementation of m26ps4's G3/G6 seams.
The branch contains the actual CreateOrder and Login SignStep implementations.
CreateOrder requests PublicAddress and Secp256k1Digest over one signing ref,
verifies both against the trusted expected account, and builds the native body
only after receipt, low-s signature and recovered-signer validation. FrozenOrder
holds the canonical digest and all once-materialized fields; prepare performs
fresh provider work without renewing nonce, expiry or client identity. Login
uses only Present for an installed credential and emits the native login wire.
Neither recipe holds the API key or scalar outside the outbound presentation.

The native placement wire types are shared by the recipe and existing caller;
removing the added visibility and Clone/Debug derives reconstructs the original
type block exactly. Native eager signing, login, queue, entry and constructor
selection are not switched. Options still belongs to that unconverted native
path; neither new recipe reads it or falls back to it. No inventory entry is
pruned. The only Cargo change enables the existing local-provider secp256k1 arm
for dev-dependencies so the isolated recipe controls use the real LocalSigner.
Production feature adoption, engine, host and registry remain unchanged.

The new engine-driven recipe controls live in tests/support/provider_recipes.rs,
following the repository's test-only module layout. Fixture keys and oracle
signing stay outside production source; the signing inventory is unchanged.
They cover exact local wire on both domains,
ordinary Stale with immutable bytes, asynchronous external signatures that vary
only r/s/v, wrong signer/digest/high-s/receipt/header rejection, Present-only login
wire, and expiry without provider work. These controls use real admitted owners,
Prepare, Bind, Claim and asynchronous Signed delivery. Login's installed-credential
probe selects SessionAuth and uses a real Session context. It is not proof of the
Empty-intent constructor, Optional absence, native queue or serial resend adoption.
Their execution receipt is recorded after the scoped run, rather than inferred
from the pre-existing funnel originals.

G3/G6 remain the stopping boundary: no fake Identity, empty signing-ref sentinel,
caller-supplied setup facts, Empty Present operation, account-latch override or
native fallback. Actual constructor cutover needs #2149's shared source and its
consumer controls. Later adoption must explicitly enable the reviewed features,
run the GRVT size/budget gate, retain bf61 originals, and obtain source reviews.


Local verification: 13 library tests (the six new recipe controls plus seven
existing library controls), 42 unchanged trading controls and one security
control pass: 56 total, none ignored. After the test-only module moved into
support, all six recipe controls and strict GRVT lib/tests clippy passed again.
Formatting and diff checks pass. The source gate retains the old bf61 target
byte-for-byte; its intentional custody REDs are not run or called green here.
The isolated LocalSigner arm initially refused Unsupported until the explicit
dev-only secp256k1 opt-in; the positive tests expose that condition. The async
driver then uses actual Signed ingress with a retained prepared route instead
of the immediate-only fixture. Neither change replaces provider work or Claim.
The initial signing-ratchet failure identified test fixture secret setup in
src; moving that unit module into tests/support follows the existing test-only
layout, with no inventory addition or checker change.


Full bans on this frozen local parent fail only the foreign stale OKX request
keys in lib.rs/trading.rs and credential-selection lib.rs against the advancing
origin/feat/rust-rewrite. The current request scan matches its checked-in
inventory; no GRVT site or map is added or pruned. Signing, socket and checker
unit controls pass. This is a recorded full-gate failure, not a complete bans
PASS or permission to change foreign maps. Recompose and rerun on the adoption
tip after the engine freeze and shared seams are resolved.

The overseer reports #2132 landed at a2c914e2b24466725c546c87cdfb157797fbec0e;
that object is verified locally. It introduces explicit optional signing refs,
so the eventual composition must use the landed required-signing accessor for
CreateOrder rather than the pre-D1 field shape compiled at this frozen parent.
G3/G6 in #2149 is still the next shared gate. No newer engine input is silently
merged into the published #2146 A3d30/B7aa diagnostic pair, and no compatibility
shim or optional-signing emulation is added to this recipe checkpoint.


Plain-merged 702afdffef3759bd0bab5502774ce55ba7ae1a18 locally after the
explicit stale-base instruction. Runtime composition was automatic; README
and CALLERS retain the entire incoming file plus the unchanged own suffix.
CreateOrder now obtains its required ref through the landed signing_ref()
accessor, the one mechanical adjustment to #2132's optional-ref representation.
There is no signing_opt(), empty sentinel or G6 latch workaround. The provider
recipe remains a signed-operation API; credential-only login selection and
installed native cutover remain at the #2149 boundary. The 8c695 receipt is the
pre-composition execution receipt; it is not a new composed-test claim.

### Landed resend and credential-only recipe refusal

The local recipe branch now contains actual #2146 landing f895d0d7018. Its
engine, scripts and harness are exact incoming; the merge resolves README and
CALLERS only, as incoming plus the unchanged GRVT suffix once. This does not
activate the native GRVT funnel or claim G3/G6 source has landed.

CreateOrder uses the landed D1 signing_ref() accessor. A new control installs a
real credential-only primary through trusted Optional setup, verifies its valid
credential and absent signing ref, and requires the order recipe constructor
to refuse InvalidRequest before producing work. It invents no credential or
signing ref. This is a pure recipe admission control, not an Empty login intent,
engine G6 journal/latch control or installed GRVT constructor certification.
G3/G6 and native queue/login cutover remain held; the composed scoped receipt
must pass before source-carry review.
