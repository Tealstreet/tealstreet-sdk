# Paradex local, hosted and server-subkey signing

Status: plan only, no source or implementation authorization. Requested after
#2104. Source audit: actual 76f1f7fc842; recheck the actual tip before implementation.
Owner: codex-9czdjh. Shared signer/setup reviewers: m26ps4 and the engine owner;
external hosted-signing and exchange-server work needs those service owners.

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
   available for the existing HTTP contract. If both callbacks are configured,
   route by phase; do not apply generic remote-over-payload precedence in a way
   that suppresses Paradex's required order-signing callback. Ambiguous ownership
   or unsupported capabilities must refuse before private work, never fallback
   to a local key.
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
   A generic engine PR measures Bybit and Bitget on the composed tree. Measure
   family and combined size and CPU/replay with unchanged budgets/profiles.
5. Source reviews, one intended freeze push/run, READY notification and the
   auditor's exact-tree GO remain required. This plan changes no runtime, Cargo,
   manifest, original fixture, hosted service, provisioning flow or CI.
