# Native hosted HTTP signing

The production registry retains the account's shared hosted signer in both
`BaseExchange.signers` and `native_inputs_signed`'s HTTP client. The existing
unsigned `native_inputs` API supplies `None`. This uses the common transport
signer bridge and retains Backpack's request serializer, retry policy, transport
installation and public-request skip marker.

The source permits hosted HTTP without a local secret. Its private WebSocket
methods instead call the local Ed25519 signer, without consulting remoteSigner.
The native binding preserves that split. A missing local secret is caught and
logged by each source method, and no subscription is sent. Reconstructing the
crypto adapter's error name/message prevents an extra `Error:` prefix in those
logs. No remote WebSocket branch or construction refusal is introduced.

`safe-cex-registry/tests/backpack_hosted_source.cjs` executes the actual Backpack
API, common Axios factory and remote adapter, signing module with installed Noble
Ed25519/SHA-512, private WebSocket class, and AST-selected unmodified getAccount
body. Only transport, clock, diagnostics and unused parent capabilities are
supplied. The HTTP boundary returns a callback marker header. The WS boundary
captures sends and errors; a getter proves remoteSigner is not read.

The finite source corpus has one hosted private HTTP call, two transport sends
(account and unsigned public markets), and six WS cases: authenticate, order
subscribe and position subscribe, each with an empty or valid synthetic local
secret. All six have zero remote reads/calls. Empty secrets produce zero frames
and one RangeError log each. Valid local secrets produce one subscription each,
with signatures independently verified against the literal subscribe preimage.
The receipt records Node version, architecture and V8 version.

`safe-cex-registry/tests/native_host_backpack_hosted.rs` constructs the production
Rust registry route with an empty local secret, checks typed canSign, compares
actual-source exchange/account metadata and request method/URL/baseURL, and
observes the callback header on real local HTTP. Public markets bypass the signer.
Its second test calls all three concrete private socket methods, compares all
three emitted error strings with actual source, and observes no subscriptions
or hosted calls. The source's positive local-signature cases complement the
existing native actual-crypto signing and socket suites. This does not claim a
hosted WS protocol, successful secretless private auth, live venue connectivity,
or arbitrary remote callback behavior.

Both new native tests failed against the old production implementation: HTTP
construction refused the capability, and private WS logs had an extra `Error:`
before the source RangeError. Both pass with this implementation.

`safe-cex-registry/scripts/backpack-hosted-controls.py` removes the HTTP signer,
typed capability, public skip flag and source error reconstruction separately.
Each control must compile, fail its named native assertion, restore exact
production bytes and pass again. Run it alone, with SAFE_CEX_TYPESCRIPT_PATH and
CARGO set to the same tools as the native tests.

All four controls compiled and failed with exit 101 at the expected assertion;
all four exact restorations passed with exit 0. The source oracle also passed
standalone. Layer-policy controls passed 88/88. The control transcript is
`/tmp/ydwdec-backpack-hosted-controls.log` (local, not a committed artifact).

With `SAFE_CEX_TYPESCRIPT_PATH` pointing to the installed reference compiler:

```sh
cargo test --manifest-path sdk-rs/Cargo.toml --locked --jobs 1 -p safe-cex-registry --test native_host_backpack --test native_host_backpack_hosted
cargo test --manifest-path sdk-rs/Cargo.toml --locked --jobs 1 -p safe-cex-venue-backpack --features native --all-targets -- --include-ignored
CARGO=cargo python3.14 sdk-rs/safe-cex-registry/scripts/backpack-hosted-controls.py
```

The full native-feature Backpack suite passed 110 tests across 27 target
summaries (26 nonempty targets). Both registry targets passed all three tests,
including the existing unsigned local construction. The all-feature override
audit including the ignored live-source inventory passed 6/6 with no inventory
changes or added base-delegation exceptions.

Strict host Clippy passed for the venue with native/all-targets and for both
registry test targets. Strict wasm32-unknown-unknown all-target Clippy and scoped
format checks passed. Parity reported zero findings: 7,459 rows and 17,184 public
Rust items. No workspace-wide local test run, accepted-contract exemption or
TypeScript modification was used.
