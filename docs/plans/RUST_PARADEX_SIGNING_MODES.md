# Paradex local, hosted and server-subkey signing

Status: plan only, no source or implementation authorization. Requested after
#2104. Source audit: actual 76f1f7fc842; recheck the actual tip before implementation.
Owner: codex-9czdjh. Shared signer/setup reviewers: m26ps4 and the engine owner;
external hosted-signing and exchange-server work needs those service owners.

The overseer confirms all three modes and callback ownership: remote signing is
mandatory, and unsupported signing refuses instead of falling back. Plan reviewers
are m26ps4 (shared setup) and fe7lgm (engine). Hosted Stark payload signing and
server subkey provisioning live in tealstreet-next; the overseer assigns that
owner after this plan review. This PR changes documentation only.

## Required modes

All three modes remain supported targets. Remote signing is a required contract;
removing a callback, silently switching to a local secret, or making a hosted
account read-only is not an implementation shortcut.

| Mode | Engine and secret custody | Existing SDK path | Missing completion |
| --- | --- | --- | --- |
| Local browser/worker | The local process owns the Stark key; the exchange runs locally. | Rust Paradex Credentials::new signs auth/orders locally; TS SDK supports a Stark subkey or Ethereum-derived account. | Shared opaque signer setup/adoption and full local/remote parity proof; preserve the existing local path until direct cutover is reviewed. |
| Hosted signing | Exchange execution and signing authority are separate; the configured signer owns the key. The existing hosted architecture can put signing in a browser worker; hosted does not itself imply server custody. | Paradex already emits Stark payloadSigner work for auth/order hashes. The legacy remoteSigner adapter also forwards HTTP signing, but it does not supply the Stark order signature by itself. | Hosted signer/worker must implement and advertise Paradex Stark payload signing, with identity/network binding and complete auth/order/amend lifecycle support. The generic delegated provider currently advertises HMAC-SHA256 only. |
| Exchange-server with hosted session subkey | The exchange-server's trusted signer owns an already authorized Paradex subkey; the browser receives only public account/subkey metadata. | payloadSigner can keep the private key outside the exchange object; key is the subkey public key with paradexAddress and subkeyAddress. JWT auth and order work can already traverse this callback. | Provision/register/rotate/revoke the restricted subkey and inject a server-side Stark payloadSigner in trusted server construction; prove native session/JWT behavior and external signing custody end to end. A callback cannot be serialized in account options. |

The hosted signer implementation and subkey provisioning service are outside
this mirror. The reported absence of hosted Stark support is a requirement from
Sam, not an assertion that inspecting a TS type proves a remote service supports
it. Their owners must provide source pins and working integration evidence.

## What exists at the audited tip

- packages/safe-cex/src/types.ts declares payloadSigner with Stark/felt-pair
  payloads, optional remoteSigner, and the key/address options. Paradex's key is
  private in local mode and public in payload mode; that overload is observable
  and must be documented. hostedMode controls postprocessing placement, not key
  ownership or algorithm capability.
- packages/safe-cex/src/exchanges/paradex/paradex.sdk.ts selects payloadSigner
  before private-key account creation. signParadexTypedData passes the exact
  Stark typed-data message hash and expects the serialized decimal felt pair.
  Existing JWT logic signs /v1/auth and posts /auth/{subkeyAddress}; orders and
  amendments have their own typed data. The local Ethereum derivation branch is
  separate from a preauthorized Stark subkey.
- sdk-rs/venues/paradex/src/sign.rs has Credentials::new and delegated. The
  delegated form retains public metadata, returns the digest instead of signing
  locally, and emits scheme=stark, encoding=felt-pair payload work. Config
  handling in account.rs chooses the constructor from payload_signer. Existing
  delegated multi-order batch refusal remains an explicit limitation; do not
  promise local-mode batching parity without a reviewed consumer change.
- rust.exchange.ts wires signPayload when the Paradex payloadSigning rule is
  present. paradex.signing.ts wraps non-auth HTTP through remoteSigner and keeps
  request correlation/abort handling. Auth is intentionally outside that HTTP
  wrapper. HTTP signing and typed order signing are different phases: preserving
  remoteSigner alone does not implement an order payload signer.
- The shared engine has StarkDigest and local crypto support. That proves the
  primitive exists, not that a hosted worker implements it. remoteSignerProvider
  advertises only hmacSha256, while construction-rules has no generic delegated
  Paradex/GRVT/Lighter enablement. Do not enable an unsupported algorithm by
  merely broadening a TS union or copying an HMAC capability list.

## Mode selection and TS-visible contract

Keep the public exchange command/event API the same across modes. Select the
signing authority before constructing the account and freeze it for the account
lifetime. A future shared setup can represent the mode explicitly, but no new
public option is implemented or considered approved by this plan.

1. Local: no delegated payload callback, with valid local key and account data.
   The browser/worker's local signer owns the secret. No remote callback must be
   invoked and no local secret belongs in transport messages.
2. Hosted: inject a payloadSigner adapter which forwards Stark work to the
   configured signing authority. Exchange options carry the public subkey and
   account addresses, not the private Stark scalar. remoteSigner remains
   available for the existing legacy HTTP contract. Legacy TS may keep both
   adapters until cutover; that compatibility wiring must not suppress required
   Stark payload work. The shared route uses one Signer provider and dispatches
   each operation by its declared algorithm, without callback phase precedence.
   Ambiguous ownership or unsupported capabilities refuse before private work,
   never falling back to a local key.
3. Server-subkey: trusted exchange-server construction injects an in-process
   Stark payloadSigner backed by its subkey store. Browser account/wire options
   carry only public metadata. Select this through trusted account configuration,
   not key length, hostedMode, a caller-supplied claim, or serializing a function.
   Existing 66-character local-key detection must not decide remote custody.

TS-visible differences are the supplied credentials and callback, private-key
location, async signing/availability failures, public-key identity metadata and
any existing delegated batching limitation. Keep signer result encoding,
command outcomes, timestamps, financial rounding, JWT headers and selected
network unchanged. No private scalar, JWT or signature-bearing diagnostic may
be exposed as a mode/status field. Disposal, disconnect, timeout and late results
must preserve current cancellation/failure behavior; no silent startup success
followed by an inevitable unsupported signer failure.

Keep remote HTTP/WS support and public-only mode separately testable. A missing
Stark capability is not an authentication rejection from Paradex. The precise
shared typed setup/admission error and callback-combination rules need owner
review before source. Existing valid remote configurations cannot be dropped.

## Shared setup, admission and lifecycle pins

The shared setup/transport plan (delivery step 2) must resolve these pins using
existing constructs. The mode matrix authorizes no new callback or provider API.

| Mode | Shared construction target |
| --- | --- |
| Local | Trusted LocalSetup installs opaque refs into Rust LocalSigner, declaring StarkDigest on the signing ref through IdentitySetupPolicy.signing_algorithm. |
| Hosted | Inject Identities through EngineOptions.signing; injected slots carry valid signing refs, and the host Signer advertises StarkDigest for the selected ref. Do not install a local private key in this mode. |
| Server-subkey | Trusted native build_with_signer construction supplies an in-process Signer backed by the authorized subkey store. Do not serialize a callback or private scalar. |

Action::Sign / Signer is the single shared provider surface. A provider dispatches
HmacSha256 and StarkDigest operations by algorithm-declared refs. The current
hosted Input::Credential/Http/WsAuth and remote_signing_policy are HMAC-shaped;
Stark adoption uses the shared Sign route, or an independently reviewed new
Input. Extending a HMAC policy table does not add hosted Stark support.

Identity.validate requires a valid credential ref as well as a signing ref.
Paradex has no API-key credential, so the shared setup owner must define the real
credential-presentation subject (for example an account/public-subkey reference
used by auth) before source. This remains OPEN; no empty or fake ref is allowed.
Public key/addresses are public metadata, not refs to secret material. Shared
setup selects by trusted mode and eliminates the legacy private/public key
option overload. Startup verifies the configured public subkey by issuing
Operation::PublicKey { algorithm: StarkDigest, key: selected signing ref } and
comparing the provider result to the account/network's expected public subkey
before any private work.

Algorithm admission has two lines. At genuine command or Preflight admission,
check the selected ref's declared StarkDigest against the provider capability
through Cx::can_sign, before queueing or provisional effects; Unsupported refuses
with NotSent. Prepare checks again. At this tip, signing_algorithm is a trusted
IdentitySetupPolicy field, not an Identity field, and runs only for trusted
LocalSetup. Hosted injected Identities carry no setup policy: host declarations
through set_host_algorithms supply capability for their refs. Server-subkey
construction gets it from the in-process Signer::can_sign for the selected ref.
These are Cx::can_sign's per-mode sources, not serialized policy on Identities.
Paradex is a Required signing family: auth itself needs
Stark, so absent capability means no private Session can be created. Setup or
construction refusal is consistent with GRVT G6's per-operation rule because
Paradex has no credential-only/Optional private path. The landed #2109 MissingKey
ownership latch applies when an installed required primary Stark ref is missing.
Do not import GRVT's deliberate Optional absence exception into Paradex.

Adoption uses managed dispatch signing, receipt attestation and existing Attempt
custody for duplicate/late results. After the attempt window a signature is
Stale; the owner ends as the window's Timeout/NotSent terminal with nothing sent.
No venue-side late-result mechanism is added. Before adoption, controls must
reject a noncanonical felt pair, out-of-range r/s or a signature that does not
verify against the expected public subkey for that account/network, with NotSent.
Pin wrong account, network and public key as well as malformed encoding.

Paradex JWT maps to the landed #2069 Session bearer hook. The Stark-signed
/v1/auth operation is Session login, not a managed command. The JWT is the Bearer
bundle, its revision moves on every refresh, and session_ready follows refresh.
NotReady parks bound calls only inside their original windows. Rotation or
revocation retires the Session with retire_session; queued/parked calls end or
re-park through the shared fence and never send under an old JWT or key. Private
WS auth uses that same Session identity/lifetime; the socket owner must pin its
binding, since the REST bearer hook does not automatically bind a socket epoch.
No venue-local JWT resend/repair path replaces the shared revision fence.

Signed order/amend payloads freeze signature_timestamp and all signed fields at
native operation construction (trading.rs prepares once; account.rs queues the
same body/digest behind auth). Native writes do not gain an automatic resend from
this plan. A separately authorized retry or attempt-2 resend re-signs the exact
frozen digest/fields inside the original window; neither provider nor transport
recomputes timestamps, expiry or economics. If the original window or declared
signed validity has expired, refuse before send instead of resending a stale
timestamp. A new logical command constructs fresh fields. Controls compare the
original and re-prepared digest/body/timestamp, deny an expired resend with no
transport, and verify the provider cannot substitute fresh signed fields.

## Hosted Stark and subkey completion

Hosted payload signing must bind exchange/network/account/public subkey and
allowed operation to the authenticated account. Sign the exact digest supplied
by the recipe; do not recompute timestamps or mutate size/price/side/order ID,
expiry, amend identity, chain or domain in the provider. Verify canonical field
ranges, expected public key and signature form before adoption. Where the
shared Signer route is adopted, use opaque algorithm-declared refs, receipt
attestation and current managed dispatch ownership instead of advisory context.

For server subkeys, separate provisioning from trading. The service must receive
proof that the main account authorized that public subkey and its permissions;
retain expiration/revocation metadata, restore it on restart, and retire/refresh
JWT and queued work on rotation using reviewed lifecycle APIs. Do not invent an
on-chain registration endpoint or claim the SDK already provisions subkeys.
Define storage custody, authorization, account mapping and revocation behavior
with the service owner before implementing source. The local browser key is not
uploaded as part of choosing this mode.

End-to-end proof must cover local and hosted auth, JWT refresh, private WS token
use, ordinary order and amend, cancellation, reconnect, signer unavailability,
wrong account/network/key/digest, duplicate/late results and disposal. Compare
local vectors against an independent Stark implementation. Verify actual final
sent auth/order bytes and financial fields, not just an intermediate callback.
Remote callback delay stays inside the original caller/physical deadlines.

## GRVT and Lighter: related remote gap, different cryptography

| Family | Actual primitive | Current distinction | Missing remote work |
| --- | --- | --- | --- |
| Paradex | Stark ECDSA over the Stark typed-data hash | Existing legacy Stark payloadSigner in TS/Rust | Hosted Stark implementation plus safe shared-provider/constructor and subkey service adoption |
| GRVT | EIP-712 with secp256k1; not Stark | Native Rust sign.rs signs locally; key-only login and unsigned cancels/leverage are distinct from signed order eligibility. Current private payloadSigner/delegated path is refused. | Reviewed GRVT provider-recipe/setup work, hosted secp256k1 digest signing, correct signer recovery/low-s/result validation and immutable resend fields. Do not route it through felt-pair or a fake scalar. |
| Lighter / Robinhood | ECgFp5 Schnorr with Poseidon2; not Stark | Rust has SchnorrDigest and local transaction/auth support. remote_signer admission/read identity is not proof of remote transaction signing. | Hosted ECgFp5 capability and native transaction/auth-token provider integration, exact account/API-key-slot/chain/nonce/entropy binding, plus public construction and real-family proof. Do not treat remote reads as trading support. |

The shared issue is that an HMAC-only remote adapter does not implement these
algorithms. A Paradex Stark addition must not advertise GRVT or Lighter as
supported. Coordinate with zs5e83's RUST_GRVT_PROVIDER_RECIPE.md and m26ps4's
credential-only setup work; preserve GRVT's unsigned key-only operations and
Required-versus-Optional missing-key rules. Lighter needs its own algorithm owner
and nonce/session fit, not a venue-specific Stark workaround.

## Plan-first delivery and review gates

1. Confirm this mode matrix and callback ownership with the overseer and shared
   signer owner. Obtain external hosted and exchange-server owner/source pins.
2. Plan shared capability/algorithm transport and trusted setup changes separately
   from venue adoption; retain remote signing throughout. No copied provider,
   issuer, fallback allocator or fake Session is authorized.
3. Write original controls for all modes and unsupported configurations before
   source. Exercise real local and external signers through actual EngineHost /
   WASM and final transport, including server-subkey rotation and stale results.
4. Implement only after setup/algorithm/session/venue source scopes are reviewed.
   Paradex managed Stark adoption measures Paradex family, combined, Bybit and
   Bitget on the composed tree, even when the consumer is the entry to new engine
   paths. Preserve fixed budgets/profiles and require qualifying CPU/replay.
5. Source reviews, one intended freeze push/run, READY notification and the
   auditor's exact-tree GO remain required. This plan changes no runtime, Cargo,
   manifest, original fixture, hosted service, provisioning flow or CI.

## Bounded plan review

m26ps4 and fe7lgm independently CLEAR the plan-first direction at 381eadfc2bb.
This supplement folds their shared-construct, algorithm-admission, public-key,
Required-family, Session/JWT, managed receipt/late-result, immutable resend and
composed-size pins. The credential-ref meaning and external service source pins
remain explicit setup/implementation gates; neither review authorizes source.
