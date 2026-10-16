# Registry data

The behavioral reference is [the exchange guidance](../../packages/safe-cex/src/exchanges/CLAUDE.md).

`registry` holds ordered descriptors from both TS registries; `loaders` holds
48 executable web/native pairs backed by the explicit `module_host` contracts.
Descriptors support inspection without invoking hosts. Constructor capabilities
call supplied factories; concrete venue host/facade assembly is separate.
The SDK facade is one layer above registry.

`rust_venues` maps bound venue names to Rust constructors, chosen per name by
`ConstructionPath::Rust`. The default, `ModuleHost`, is today's route, and the
Rust path never falls back to the module host. See [CONSTRUCTION.md](CONSTRUCTION.md).

`safe-cex-types::gateway` holds the shared gateway wire declarations below all
venues. `public_data` holds thresholds and candle subscription-key mapping. Provider lifecycle, socket IO, Store writes and delegation are implemented
against supplied parent/socket/timer hosts; concrete assembly remains separate.
Optional gateway fields preserve omitted versus present-null `unknown` data.
Specialized update envelopes enforce their literal type and channel tags.

Run the scoped test files covering a change and `yarn registry:check` from root.
The live oracle is mandatory in the Rust workflow types job after the root
dependency install; the CI wiring audit rejects removal or failure masking.
Keep registry-data descriptors separate from the parallel `safe-cex-exchange/src` trait.

Malformed no-delimiter keys ending in an astral character are explicitly
divergent: TS slices UTF-16 and produces a lone surrogate, while Rust strings
truncate a Unicode scalar. Valid generated colon-bearing keys preserve Unicode.

GatewayChannel explicitly preserves the TS unchecked `auth` cast as a declared
runtime divergence. GatewayMessage validates the entire frame, unlike the unchecked TS cast: missing
required fields, unknown enums, wrong types and typed optional nulls reject the
whole frame, even when a handler would ignore the offending field. This broader
validation difference is declared divergent. Four envelope constructors permit external producers while
keeping literal wire tags enforced. The ambient native `require` declaration is not-applicable; executable native
entries delegate through NativeModuleHost.

The native registry's `load` wrapper is independently implemented as a reusable
lazy callback returning `Ready<T>`. It evaluates the callback synchronously on
invocation, before returning the future, including synchronous host failure.
Native future polling replaces JavaScript Promise microtask scheduling and is
an explicit CHOSEN boundary. This wrapper is distinct from the executable venue loader entries and does
not complete any venue constructor. `tests/native_load` executes the
actual TypeScript initializer and observes native constructor identity and call
order; the 48 entry functions in each registry have their own source/native witnesses.

Gateway opaque data and the raw provider frame use canonical retained JSON capture
before Value construction. The public-data provider gate asserts literal reserved-
looking info objects through both entrypoints before source/wire comparisons.

Registry object/export/continuation choices are priced once at
[the canonical registry choice](../parity/MANIFEST-SPEC.md#registry-loader-choice).
The choice supplement executes actual source scalar/thenable admission, object
mutation and discarded Promise continuation; native cancellation is measured
before source capture. These do not claim a runtime export validator.

PublicDataSnapshot.loaded is an Rc<RefCell<StoreDataLoaded>> handle, like the
source's retained loaded object. In-process seeding rereads that handle after
parent notification; copy it only when constructing a parent update. The
reentrant seed witness is in tests/public_data_providers.rs. Provider policy
choices cite MANIFEST-SPEC.md#public-data-provider-choice.

`native_host::native_venue_host(limiter)` is the production `RustVenueHost` (direct
transport, real HTTP); `native_venue_host_with` takes a runtime and adapter
factory. Native registry builds always carry it, since a feature cannot be
enabled for this crate's own tests without a lateral self-edge; wasm builds do
not. `tests/native_host_binance.rs` constructs Binance through
`rust_constructor` against local HTTP/WS servers and verifies the HMAC
server-side; `native_host_backpack.rs` reads markets through the real adapter.

The native host builds `externalPublicData` providers for venues, which cannot
reach this crate: the websocket gateway runs `ExternalPublicDataProvider` over a
native `GatewayHost` (runtime dials and timers) with the exchange Store as parent;
the gateway always delivers whole batches, so the owned Store copy is current.
The in-process variant cannot be named in native options (its capability type is
uninhabited) and fails visibly until a host-registered source exists.
Every venue takes the provider through js_runtime's `ExternalPublicDataFactory`,
called with the exchange's own `Rc<BaseExchange>`, so OKX, Bitget, BloFin and
Bitunix share Binance's gateway path. `tests/native_host_external_data.rs` starts
Binance and OKX on a local gateway socket.

`native_venue_host_signed` is one account's host with a hosted `remoteSigner`
(exchange-server's browser relay) and its accountName; no secret is held. Binance
installs the remote signing adapter on every client and signs Spot ws-auth
through it. OKX-family, BloFin and Bitunix also share that hosted account for
HTTP and private websocket auth. Bybit and Bybitdemo use the same hosted signer
through their typed API factory and source receiver; the native HTTP witness is
`tests/native_host_bybit.rs`, with no local credentials. Coinbase and Coinbase Spot use the same shared
hosted signer for HTTP and native JWT websocket subscriptions. WOO is registered
through its existing production binding and the same signed HTTP host; its local
account-read test verifies the actual HMAC preimage. The JS host binds its exchange name during
BaseExchange construction and injects the same signer into options and core
availability; it refuses reuse for a different exchange. Bitunix retains the
account name in its private auth request. Backpack shares the hosted HTTP signer
while its private websocket retains the source local-secret requirement;
empty-secret authentication logs the source error and sends no subscription.
`tests/native_host_remote_signer.rs` verifies the signer's HMAC server-side.
`tests/native_host_tunnel.rs` runs `tealstreetTunnel` REST over a local tradetunnel
(rpc.auth with the proxy token, rpc.http frames) with no direct venue request.
`tests/native_host_binance_family.rs` runs BinancePMExchange (PAPI, listen-key stream)
and BinanceSpotExchange (signed ws-api user-data subscription) on the native host.

The native JSON account surface forwards journal reads (`fetchHistoricalPositions`)
to the venue's declared fold/reported path, preserving account and time bounds.
It must not inherit a TS wrapper's default rejection for a supported venue method.

GRVT is constructed by the Rust table through `RustVenueHost::grvt`, with a fresh
terminal HTTP host and the common public-data provider. Its HTTP clients receive
the shared hosted signer, including untagged public requests. Active GRVT SDK
order signing still requires its local wallet; the hosted HTTP capability does
not replace it. The hosted signer witness compares live source request inputs,
real HTTP headers, callback refusal and the pre-HTTP SDK wallet guard.
The registry construction test reads actual GRVT public-gateway market data.
BingX is constructed by `rust_constructor` through its shared-js_runtime binding.
The native host supplies a fresh HTTP host and the common gateway factory; remote
signer hosts fail visibly until BingX signing is connected.
`tests/native_host_bingx.rs` starts it on loopback HTTP/WS, verifies a signed order
payload and caller client ID, and cancels the resulting order.

Lighter and Lighter Robinhood construct `LighterNativeExchange` through the Rust
table. The native host gives each instance a transport registry seeded from its
runtime, fresh terminal HTTP and OS signing entropy. Hosted remote signing is
refused because this binding uses the local SDK signer. Construction tests cover
both names and their deployment-specific REST origins and quote currencies.

Hyperliquid is constructed through the native Rust table with options-aware
transport composition. All native host constructors require the same caller-owned
`Rc<HyperliquidRateLimiter>` across accounts. The FFI command LocalSet creates one
via `hyperliquid_rate_limiter()` and forwards it through `NativeAccountSpec`;
never create a fresh budget per hosted account. The Hyperliquid adapter preserves
the transport-resolved baseURL, repeat query serialization, hosted action signing
and info skip-signing. Its local HTTP proof is `tests/native_host_hyperliquid.rs`.

KuCoin futures/Spot and MEXC registry entries reuse their concrete constructors.
Their native host inputs retain each API's signing/transport/retry middleware.
Private MEXC fills must reach the venue emitter; retain the weak callback when
adjusting constructor inputs. `native_host_kucoin_mexc` verifies each family's
native request signatures against a local server.
KuCoin futures/Spot also retain the shared hosted signer and account name in
both HTTP clients and BaseExchange availability. Private WS uses its HTTP
bullet-private token, as in TypeScript. `native_host_kucoin_hosted` verifies
account and unlimited HTTP plus real private subscriptions without a local secret.
MEXC shares the hosted account with HTTP, BaseExchange availability and private
WS login; preserve the private socket account name. `native_host_mexc_hosted`
compares actual source success/error/missing-signature outcomes over native HTTP
and WS.

Crypto.com constructs through `cryptocom_constructor` using its bound Js runtime.
Native HTTP inputs carry the account's hosted signer and supply the existing
external-public-data gateway factory. The native host
constructor still takes the caller-owned Hyperliquid limiter introduced by the
shared host API. `tests/native_host_cryptocom.rs` constructs through the registry
and production native host, then reads markets over real local HTTP and checks
read purity. The module-host construction path remains the default.

Phemex's Rust constructor receives the shared Js HTTP host and external-data
factory. Its local HTTP witness checks amount/price precision independently and
verifies the account-query HMAC against the bytes observed by the server. Native
Phemex tunnel construction supplies the shared default transport installer.
The tunnel witness observes authenticated rpc.http, signed account bytes and zero
direct-origin traffic; absent and no-op provider controls must both fail.

Kraken Futures/Spot native inputs use the shared hosted signer bridge for private
HTTP and Futures websocket challenges. Both retain the hosted account name with
empty local secrets. The native witness verifies signed HTTP bytes and challenge
subscription fields; the account metadata comes from live injected options.

ApexOmni now has its own Rust table entry and native host factory. It uses the
shared js_runtime binding and safe-cex-crypto's real seed-only zkLink signer.
`tests/native_host_apexomni.rs` covers signed startup, placement, private WS order
delivery and cancellation over local native transports; its constructor-removal
control lives in the venue's scripts/registry-control.py.

The Bybit host also constructs `Bybitspot` and `Bybitinverse` through their
distinct retained receivers. Native Bybit loopback HTTP coverage includes both variants for local and hosted
signing; its existing registry dependency edge and caller-owned limiter API
remain shared across these constructors. HyroTrader awaits its binding. The Inverse registry removal control compiles,
then fails both local and hosted construction assertions before exact restoration.

ApexOmni's signed native host shares the account capability with HTTP and private
WS auth, retaining remote accountName while keeping the API secret in the browser.
`native_host_apexomni` verifies both signed wire paths and pending-signature
cancellation; the Omni seed remains necessary for source L2 contract signing.

Bitget, Bitget USDC and Bitget Inverse native constructors supply the common
gateway factory through their existing `external_public_data` dependency. The
loopback `native_host_bitget_external_data` test covers all three names: gateway
market/ticker startup, book/candle callbacks, unsubscribe cleanup and disposal,
with no venue REST or private/public venue sockets in public-only mode.


Paradex's native constructor supplies shared runtime HTTP and gateway inputs.
Its loopback test observes distinct market precision axes and source read purity.
Hosted remoteSigner remains an explicit refusal until SDK signing is bound.

WEEX is registered through its concrete native binding and the shared signed HTTP
host, gateway factory and UTC host date profile. `tests/native_host_weex.rs` checks
its account read and exact request HMAC over loopback HTTP.

TxFlow is constructible only as the source Phase-0 scaffold. Native inputs build
its HTTP client, while start emits the exact refusal before IO. Keep its existing
source start boundary and test both mainnet/testnet construction without network.

Breakout Prop constructs its DXtrade API/session/polling lifecycle through shared
js_runtime. The native host preserves the selected transport and source session
headers; remoteSigner DXtrade login refuses visibly. The native date realm is
explicitly UTC for offset-less legacy timestamps, while embedding hosts supply
their own local_to_utc policy. native_host_breakoutprop.rs proves actual native
HTTP login/startup/reads/placement/cancellation with no websocket dial.

Orderly-family per-options HTTP hosts share the hosted signer and account name,
with the selected family name bound before requests. Preserve the source local
signing requirement on private HTTP even with hosted signing. Private WS stays
local-only; empty-secret recovery follows the existing approved onOpen contract.
`native_host_orderly_hosted` covers all six names with valid/empty local secrets.
