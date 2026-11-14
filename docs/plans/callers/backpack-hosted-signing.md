# Backpack hosted signing

Sam approved this production signing activation; review is required before merge.
Backpack perpetuals only; private PayloadSigner remains unsupported.

## Admission and custody

The venue roster, metadata and WASM family already admitted Backpack locally.
`assertBetaAccount` refused Hosted accounts because construction-rules.json had
no enabled `remoteSigning` entry. That one-line declaration now admits
RemoteSigner and declares Ed25519/base64 provider setup. The user-facing message
in betaVenues.ts is unchanged. Undeclared venues still refuse before callbacks.

Admission alone was insufficient. The registry rejected injected providers and
private startup depended on raw key/seed fields; private WS signed eagerly.
Registry construction now consumes both credential slots into provider custody
and passes only opaque identities and read-key validation facts to the venue.
Account/history reads, writes, cancel/proof/replacement amend, trigger repair
and private WS all use the owned provider. Malformed local seed widths refuse
before startup I/O. Public-only saved-read-key behavior remains intact.

The public direct `Backpack::new(Options)` compatibility API keeps its existing
raw-key setup and eager WS path for direct callers and frozen replay controls.
Production SDK/registry engines have no raw key or seed in their venue options.
Installed refs alone do not confer account-repair readiness: native account
startup must have run. Pre-account history/identity reads retain the native
60,000 ms default; after account startup the configured receive window applies.

## Device protocol

The existing ws-auth wire gains `outputFormat: backpack-ed25519` and a closed
JSON `toSign: {slot, payload}` descriptor. `slot` is primary or secondary;
`payload` contains the exact native canonical bytes. The device resolves its
own public credential and signing ref atomically into `[Present, Sign Ed25519]`
and returns a JSON string `{credential, signature}` with canonical padded
base64 for exactly 64 signature bytes. No seed or opaque ref crosses that wire.
The origin binds the credential to its selected slot and validates the complete
reply before publishing any value. A raw HMAC output format cannot substitute.
The native format is compiled only with the `backpack` feature so other venue
families retain their existing format decoder.

Missing device custody, foreign refs, unsupported algorithms, partial graphs,
malformed/wrong-account replies, unavailability, cancellation and callback
errors refuse without local or PayloadSigner fallback. Errors discard callback
text; fixtures check errors, logs, generic options and the remote wire for
fixture secret material. BaseExchange refuses this format unless the Rust
device codec owns it.

## Parity evidence

RED regressions reproduced missing beta admission, missing Ed25519 capture
format, rejected provider injection and missing private-capsule seed-width
validation. The fixes preserve independent negative controls.

- `rust.backpack-hosted-signing.test.ts` tests unmocked admission and callback
  refusal, all canonical payloads and OpenSSL Ed25519 signatures, both fixed
  and corrected timestamps, malformed answers, cancellation and redaction.
- `registry/tests/backpack_hosted_signing.rs` tests actual Ed25519 custody,
  startup reads and WS without raw credentials, closed descriptor decoding,
  missing slots, clock-revision fencing, timeout, malformed completion and fresh
  re-signing. Capture tokens are consumed even on refusal.
- The native fixture provider in `tests/support/hosted_parity.rs` compares the
  actual dispatched graph and all result bytes with the hosted capture. Existing
  ordinary/protective placement, cancel, cancel-all, amend, trigger repair and
  error/Unknown controls keep their existing wire assertions and scheduling.
- `rust.backpack-hosted-provider.wasm.test.ts` runs actual local/device and
  keyless remote/origin WASM. Fifteen REST vectors run at two fixed timestamps;
  request configs, canonical payloads and signatures agree with independent
  Node/OpenSSL signing. It also exercises private WS, secondary custody,
  malformed formats, failures, no fallback, redacted logs and native startup.
  No live network calls or orders are used.

The complete signed method/path inventory is GET account, position, collateral,
open orders, and fills/order/funding histories; POST order and batch orders;
DELETE order and symbol orders; PATCH account leverage. Ordinary and protective
amend use cancel, open-order proof and replacement placement, not another amend
endpoint. Private WS signs `instruction=subscribe` over the same Ed25519 graph.
REST defaults to 60,000 ms (configured windows clamp to that maximum); WS uses
5,000 ms. Backpack has no signed nonce. Fixtures fix clocks, deterministic
entropy/client IDs and receive windows; changed clocks produce fresh signatures.
Physical transport loss remains Unknown and never replays a write.

CI and maintainer review remain authoritative. Family budget changes, if needed,
belong in a separate final commit using CI size plus 10 KiB.

## Native hosted provider

The native remote-signer selects a Backpack Ed25519 family instead of its HMAC
fallback. It accepts only an owned credential presentation or an atomic owned
`[Present, Sign Ed25519]` graph with one bounded, nonempty UTF-8 payload. It sends
the existing primary-slot device descriptor without rebuilding the canonical
payload, and preserves the original receipt. The reply must bind the installed
public credential and contain exactly one canonical base64 64-byte signature;
duplicate or unknown fields refuse before any result is published.

`hosts/remote-signer/src/tests/backpack.rs` pins six signatures from the existing
TypeScript `backpackHostedFixtures.signature` Node/OpenSSL oracle: account read,
batch place and WS subscribe at two fixed timestamps. A real `SignerLink`
round trip uses the existing registry device codec and compares every result
byte with local signing. Refusal controls cover malformed graphs/replies,
callback redaction, cancellation, timeout and socket ownership. Signer requests
may move to a resumed socket; that is not an exchange write retry.

This slice changes no admission flag, construction rule or venue capability
manifest. Native Ed25519 capability ships with its operational provider branch.
NEEDS-NX: caller tests using Backpack as a native Hosted refusal example need
integration verification. Native host integration is described below;
native secondary custody and private PayloadSigner stay refused.

## Native host integration proof

The native REST-base resolver must name Backpack as well as the registry and
signer provider. It now selects `https://api.backpack.exchange` for production
and refuses testnet rather than sending a testnet account to production.
Keyless SDK setup resolves public mode before keeping delegated authority,
using the shared normalized-presence rule being integrated in #3320. It retains
no primary identity, signing algorithms or callback subscriptions in that mode.
No construction-rule, beta message or capability-manifest value changes here.

`hosts/server/tests/backpack_hosted.rs` drives `accounts::prepare`, the injected
registry engine, actual `SignerLink` request/response frames and the existing
device codec. Its startup proof also runs the native `Host` with a prepared and
bound recording transport through final handoff; no network executor is used.
The fixed-clock driver keeps prepared resources across asynchronous signatures,
checks final release claims, and fences stale results after a clock revision.

Ninety-six REST cases pin every supported signed endpoint plus escaped queries,
at two fractional clocks and three window settings, to literal signatures from
the TypeScript Node/OpenSSL oracle. Two WS goldens retain its separate 5-second
window. Real account reads check configured-window flooring and clamping;
pre-account history and identity reads retain their existing 60-second behavior.
Server init still does not forward a custom receive window: that optional caller
wire extension is outside this proof, which supplies custom engine options.

The ambiguous-write matrix covers single/batch placement, cancellation,
cancel-all, protective amend cancellation/replacement/proof/put-back and child
legs, including late acknowledgements and prior IDs. A newly exposed transport
loss or native error envelope (including HTTP 200) on an optional child keeps
the engine's classified `Unknown` outcome and stops the serial plan, preserving
accepted entry IDs. It cannot advance to the next child. Definite optional
refusals keep their existing handling. Pre-transport refusal remains `NotSent`.
Private PayloadSigner, native secondary custody and undeclared remote venues
stay refused; public accounts open no private socket or signing channel.

NEEDS-NX remains: this runtime routing repair changes a native Hosted refusal
example. Review and integration are required; this evidence does not authorize
a live-key smoke, deployment, or merge during the freeze.
