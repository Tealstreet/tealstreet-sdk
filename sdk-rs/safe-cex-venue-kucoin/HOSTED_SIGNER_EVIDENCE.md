# Native hosted signer for KuCoin futures and Spot

The native registry supplies one HostedSigner/accountName through both the
per-options HttpClientOptions builder and KucoinDependencies. The concrete
ApiHost exposes it to the shared BaseExchange bridge; that bridge populates
typed availability and the source options.remoteSigner object. Every fresh
per-options API host retains the account capability. Local-only callers supply
None. Existing public constructors and local signing bodies are retained.

KuCoin's private sockets obtain bullet-private tokens through their HTTP client.
They do not implement a separate WS signing RPC. This path, including the token
in the native WS URL and source private topics, now works without a local secret
or passphrase. Both primary and unlimited HTTP clients use the common hosted
bridge. The source's public HTTP behavior is preserved: the hosted adapter still
observes the public market request, including Spot's early venue-interceptor
return. No Backpack-style skip flag is added here.

`safe-cex-registry/tests/kucoin_hosted_source.cjs` executes actual KuCoin and Spot
API/broker modules, the common Axios factory and remote adapter, real installed
Axios/retry/HMAC/qs, complete private socket classes, and AST-selected unmodified
getAccount/cancelAllOrders bodies. The supplied capabilities are transport,
clock, diagnostics, disabled reconnect-snapshot observation and unused socket
parent methods. Private startup, subscription and ping bodies are unchanged.

The source corpus contains two venue cases. Futures emits five signing RPCs and
five HTTP sends (account, public markets, private token, two unlimited cancels),
six private subscription topics and one ping. Spot emits four RPCs/sends, two
private topics and one ping. Both API clients are distinct. A callback marker
reaches every observed HTTP request, and one token-bearing WS dial occurs per
case. Node version, architecture and V8 version are recorded in the receipt.

`safe-cex-registry/tests/native_host_kucoin_hosted.rs` constructs each name through
the production native registry with empty local secret/passphrase. It checks
typed canSign and account identity, then runs actual startup and unlimited
cancellation over local reqwest/tungstenite transports. It compares the observed
account, private-token and unlimited-cancel RPC metadata and method/URL/baseURL
with actual source, checks callback headers on real HTTP, and observes the
private token URL and every source subscription topic and flag. An independent
assertion rejects local empty-secret KC-API-SIGN headers before the callback.
Public/startup traffic can interleave; the source subset is identified by exact
method/path and fixed per-path counts, rather than assuming a global IO count.

These are capability, metadata, returned-config and native protocol witnesses.
They do not claim real browser-held keys, live KuCoin connectivity, JavaScript
job scheduling or all arbitrary signer callbacks. Existing venue crypto/API and
constructed-binding suites retain their separate coverage.

Both new native tests compiled and failed against the old production host's
explicit remoteSigner refusal. Both pass with the bridge connected.

`safe-cex-registry/scripts/kucoin-hosted-controls.py` independently removes HTTP
signing, the typed/source account capability and the API's local-signing bypass.
Each mutation must compile, fail both named venue tests at their behavioral
assertions, restore exact production bytes and pass both tests again.

With SAFE_CEX_TYPESCRIPT_PATH pointing to the installed reference compiler:

```sh
cargo test --manifest-path sdk-rs/Cargo.toml --locked --jobs 1 -p safe-cex-registry --test native_host_kucoin_hosted --test native_host_kucoin_mexc
CARGO=cargo python3.14 sdk-rs/safe-cex-registry/scripts/kucoin-hosted-controls.py
cargo test --manifest-path sdk-rs/Cargo.toml --locked --jobs 1 -p safe-cex-venue-kucoin --all-targets -- --include-ignored
```

All three production controls compiled and failed both tests with exit 101;
all three exact restorations passed both tests. Complete assertion lines were
also independently checked for each venue label. Local transcript:
`/tmp/ydwdec-kucoin-hosted-controls.log` (not committed).

The complete scoped KuCoin suite passed 183 tests across 54 target summaries,
including all targets selected by venue-oracle:kucoin and all five constructed
binding tests. Both local families retain all sixteen entrypoints and
secondary-read/primary-write credential routing with hosted_signer None.

Final scoped checks also passed: four registry tests (both hosted families plus
existing local KuCoin/MEXC cases), strict host and wasm32 all-target KuCoin
Clippy, strict registry-test Clippy, formatting, zero parity findings (7,459 rows;
17,198 public Rust items), and all 88 layer checks. The shared all-feature,
include-ignored override audit passed five of six tests; its only finding was
the already-routed BreakoutProp inherited-socket dispose review row. No KuCoin
finding or exemption was added.

After merging master (merge head 8b2d574900), the same four registry tests and
all six shared override audit tests pass, including the repaired BreakoutProp
review row. The refreshed parity check also reports zero findings.
