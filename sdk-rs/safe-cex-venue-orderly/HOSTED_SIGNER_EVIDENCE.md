# Native hosted HTTP signing

The registry's Orderly host uses the existing shared signed HTTP bridge for all
six names. Every per-options host binds its selected name and retains the same
hosted account. The existing source API serialization, local signing, transport
selection and caller-owned limiter are preserved. A serialized remoteSigner
marker without an actual native capability retains a visible refusal.

TypeScript's API always signs private requests locally before the hosted adapter,
even when remoteSigner is present. Its private websocket never reads that signer.
This source limitation is preserved: valid local secrets enable hosted private
HTTP plus local WS, while empty local secrets throw TypeError: Expected String
before private HTTP reaches the adapter and before WS sends authentication.
Public HTTP still reaches the hosted adapter; the source has no skip marker.

`safe-cex-registry/tests/orderly_hosted_source.cjs` executes the actual API/common
Axios factories, remote adapter, encryption module with installed noble/bs58,
AST-selected unmodified getAccount body and complete private websocket class.
Inputs enter at the API/socket boundary with normalized valid credentials or the
unchanged falsy empty secret. This probe does not execute the TypeScript exchange
constructor or its rate-limit wrapper. The existing class and constructor tests
retain their separate scope. Transport, clock, diagnostics, Node global/window
absence and unused socket helpers are supplied capabilities; unused helpers throw.

The source census is twelve cases: six names times two local-secret states.
Each valid case has two HTTP RPCs/sends (account and public markets), independently
verified local Ed25519 before the hosted callback, one local WS auth frame, then
three private subscriptions and ping after a successful auth reply. Each empty
case has the exact private HTTP and WS TypeError, one public HTTP RPC/send and
no WS frames. Private WS remoteSigner getter reads and WS RPCs are zero. Source
Node/V8 versions and architecture are recorded; fixture seeds are generated.

`safe-cex-registry/tests/native_host_orderly_hosted.rs` has two test entrypoints,
each executing all six production registry names against real local HTTP/WS.
It checks account identity, typed capability, HTTP metadata/method/URL/baseURL,
returned marker headers and independently verified local signatures. A direct
private-socket connection isolates WS behavior from Exchange.start's initial
account-validation gate. Both runtimes disable reconnect-snapshot fetching.
Valid WS auth is verified against the fixed independent Ed25519 public key;
subscriptions wait for the reply. Native timestamps are checked through their
own signing preimages, rather than compared with the source fixed clock.

The existing approved divergence remains private onOpen recovery under
`safe-cex-exchange/WS_CONTRACT.md` and the BaseWebSocket.connectAndSubscribe row
in `parity/manifest/exchanges/base.ws.ts.toml`: native code catches the source
signing exception, logs the shared warning and reconnects, with no frames or
signer RPC. The TypeScript onOpen itself throws. This was accepted in #179 and
its manifest row remains divergent; hosted signing introduces no new difference.
Native empty-secret tests observe that warning, reconnect count and no frames.

`safe-cex-registry/scripts/orderly-hosted-controls.py` independently omits hosted
capability or account metadata, and injects an invented valid-secret fallback
into the local signing helper. Both entrypoints fail the shared-host omissions;
only the empty-secret entrypoint fails the fallback control. Every mutation must
compile and fail its named behavioral assertions. Exact-byte restorations must
pass both entrypoints, executing all twelve native cases again. Negative runs
abort at their first failing family case; they are not six separate mutation
runs per alias.

These finite witnesses establish capability/configuration propagation and native
protocol behavior. They do not establish live venue access, browser-held keys,
arbitrary callbacks or source event-loop scheduling. Callback markers are not
claimed to implement browser cryptography; local Ed25519 is verified separately.

Both new native entrypoints first compiled and failed the old registry refusal,
then passed all six names after the factory was wired. Three compiled controls
failed at their expected assertions: two entrypoints for each shared-host omission,
and one for the invented local-secret fallback. All three exact restorations
passed both entrypoints. The full scoped Orderly all-target/include-ignored suite
passed 83 tests across 18 targets; three registry tests also passed, including the
existing six-name local account/signature witness. Logs remain in /tmp and are
not committed. Source and native witnesses now additionally check local preimage
and signature header retention across the callback to the HTTP wire.
