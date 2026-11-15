# ApeX Omni signing and socket liveness

## Hosted and delegated signing

RemoteSigner accounts use the same dispatch recipes as local accounts for every
signed REST request and private WS login. The hosted device owns the API secret
and the 65-byte OmniKey seed; neither is sent to the origin or in a signing
request. ApeX requires both API authentication and zkLink order authorization
([official API guide](https://api-docs.omni.apex.exchange/)). No EVM private key
is needed by the running SDK after the OmniKey has been provisioned.

The origin keeps its public API key and API passphrase. The passphrase is a
protocol presentation credential, held in Present-only provider custody and
used for the required REST header/WS login field. It is absent from batch
requests and replies. Generic exchange options retain none of these credentials.

`ws-auth` with `outputFormat: apexomni-batch` carries a closed descriptor of
36-byte contract orders and canonical HMAC parts. Result references select each
contract's hex signature exactly once. The device reconstructs the complete
atomic graph, signs contracts with its OmniKey, then signs the HMAC with the
base64 **text** of its API secret. The reply contains the public credential,
contract signatures and MAC. Opaque key refs never cross this wire.

Only the contract-order zkLink transaction shape is accepted: transfers,
withdrawals, unrelated operations and malformed/foreign graphs refuse. API
keys plus OmniKey custody are the supported delegation model. Private
PayloadSigner remains `unsupported/notSent`: a single HMAC callback cannot
authorize zkLink orders. Secondary read keys are also `unsupported/notSent`,
matching the venue's existing primary-only credential model. There is no local
key fallback when a delegate fails or becomes unavailable.

Explicit `publicDataOnly: true` accounts, including those with no key, ignore
unused signing callbacks and credentials. Public mode is resolved before
private custody validation, installs no signing provider or identities, and cannot authenticate
private requests. An unused RemoteSigner or PayloadSigner cannot make a public
account require a passphrase or fail private-mode validation.

Native fixture signing compares local and device operations/signatures across
REST reads, ordinary/protective orders, cancellation, cancel-all, leverage,
cancel/proof/replacement amendments, signature-rejection re-signing and private
socket login. Tests retain the independent npm zkLink golden signatures.
Malformed/canceled delegate results cannot publish partial signing batches or
send a write; dispatched Unknown outcomes retain the existing reconciliation
and no-replay behavior. Details: `docs/plans/callers/apexomni-hosted-signing.md`.

## Socket supervision

Private sockets use `safe_cex_engine::socket::Supervisor` with a ten-second
connection deadline and a separate ten-second authentication deadline. Native
heartbeats run every 5 seconds; 30 seconds without traffic closes the socket.
Pongs do not extend authentication. Rejected login closes and retires the socket
before scheduling capped exponential backoff with jitter. Stable authenticated
traffic resets backoff; merely opening a socket does not.

ApeX keeps the production-compatible login → immediate three-channel subscribe
→ ping ordering. A successful login acknowledgement or a validated native
`ws_zk_accounts_v3` snapshot/delta establishes readiness, including wallet-only
updates for flat accounts. Wallet proof requires its account ID, supported token
and finite balance; supplied pending amounts must be nonnegative. This proof
does not change balance mapping. Malformed private rows,
empty envelopes and unrelated messages cannot cancel the authentication deadline.
An unrelated subscription rejection does not retire an authenticated socket.

The existing public socket policy remains separate. Deterministic lifecycle
regressions are in `tests/socket_liveness.rs`; private replay tapes provide
scripted evidence, not live account connectivity evidence.
