# Native hosted signing

The production registry connects the existing shared hosted signer to three
existing MEXC inputs: configured HTTP, BaseExchange availability/private WS
signing, and the private socket's account name. The API and socket signing bodies
stay intact. Private fills retain their weak emitter callback; native date
conversion and the required caller-owned limiter are preserved.

`safe-cex-registry/tests/mexc_hosted_source.cjs` executes the maintained MEXC API
factory, common Axios factory/remote adapter, actual JSON parser, installed
Axios/retry/HMAC/qs modules and complete private websocket class. Only transport,
clock, diagnostic observers and unused socket parent capabilities are supplied.
Unused utility calls throw. Node/V8 versions and architecture are recorded.

Its three source cases cover signing success, an explicit signer error and a
missing signature. Each emits one private HTTP signing RPC, two HTTP sends
(private assets and unsigned public markets) and one WS RPC. WS uses the trimmed
key plus request time, hex output and account name. Success emits one login and
the seven source private filters. Both failure cases emit the exact source
“Remote WS signing unavailable” message and no login or subscription frame.

`safe-cex-registry/tests/native_host_mexc_hosted.rs` constructs through the
production registry with an empty local secret, validates account identity and
hosted capability, and observes callback headers over real local HTTP. It
compares exact HTTP method/URL/baseURL and signer metadata with the source probe,
checks the public bypass, then starts the exchange over real native WS. The
callback signature must reach login; filters wait for the login reply. Native
request time is checked against its captured RPC/preimage/login, rather than
pretending it shares the source's fixed clock. Failures must emit the source
error and no login or private filters.

These are capability, metadata, returned-configuration and native-protocol
witnesses. They do not establish browser-held private-key cryptography, live
venue connectivity or all arbitrary callbacks. Native startup sends additional
source reads and heartbeats; the tests identify the relevant calls and frames
instead of claiming a fixed global startup count.

The permanent `safe-cex-registry/scripts/mexc-hosted-controls.py` independently
omits the HTTP signer, BaseExchange capability and WS account name. Each control
must compile, fail all three named outcome tests at its behavioral assertion,
restore exact production bytes and pass all three tests again.

Run with SAFE_CEX_TYPESCRIPT_PATH pointing to the installed reference compiler:

```sh
cargo test --manifest-path sdk-rs/Cargo.toml --locked --jobs 1 -p safe-cex-registry --test native_host_mexc_hosted --test native_host_kucoin_mexc
CARGO=cargo python3.14 sdk-rs/safe-cex-registry/scripts/mexc-hosted-controls.py
cargo test --manifest-path sdk-rs/Cargo.toml --locked --jobs 1 -p safe-cex-venue-mexc --all-targets -- --include-ignored
```

All three new native tests compiled and failed the previous production registry
construction refusal. With the capability wired, all three pass. The three
compiled omission controls each fail all three tests at the expected named
assertion with exit 101; all exact-byte restorations pass all three tests.
The full MEXC all-target/include-ignored suite passes 204 tests across 30 target
summaries. Registry tests pass 5/5, including both existing local KuCoin/MEXC
signature tests. Local transcripts are /tmp/ydwdec-mexc-hosted-*.log and are not
committed.

Strict host registry-test Clippy and wasm32 registry-library Clippy pass with
warnings denied. The test releases its RefCell borrow in an explicit lexical
block before awaiting the login reply; all five registry tests pass after that
test-only scope correction.

Final scoped gates pass: formatting; shared all-feature/include-ignored override
audit 6/6; parity zero (7,459 rows / 17,198 public Rust items); layers 88/88;
registry equivalence for all 48 ordered names plus the corruption/native-
divergence control. No dependency edges, new public venue API or manifest
classifications were introduced.

After merging master4012effa81, all seven registry tests pass (MEXC hosted,
local KuCoin/MEXC and both hosted KuCoin names). Refreshed parity remains zero:
7,459 rows / 17,204 public Rust items. Frozen dependency installation passes.
Only documentation needed conflict resolution; both families' hosted descriptions
and all master production wiring were retained.
