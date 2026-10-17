# Rust venue construction through the registry

Status: implemented for the Lighter family (`lighter`, `lighter-robinhood`), `apexomni`, `grvt`, `asterdex`, `backpack`, `bingx`, the Bitget family (`bitget`, `bitgetinverse`,
`bitgetusdc`), `bitrue`, `bitunix`, `blofin`, `breakoutprop`, Bybit (`bybit`, `bybitdemo`, `bybitspot`, `bybitinverse`),
Coinbase (`coinbase`, `coinbasespot`), `cryptocom`, `phemex`,
the Kraken family (`kraken`, `krakenspot`),
the Orderly family (`kodiakfi`, `mode`, `salsadex`, `tealstreet-dex`, `what`, `woofi`),
`hyperliquid` and `hyperliquidspot`, the Binance family (`binance`, `binancepm`,
`binancespot`) and the OKX family (`okx`, `okxdemo`,
`okxeu`, `okxeudemo`), `polymarket`, `woo` and `weex`, 2026-10-16 (`src/rust_venues.rs`,
`tests/rust_venues.rs`). Plan reference: `docs/plans/SAFE_CEX_RUST_PORT_v1.md`
(crate layout, "Constructors and loaders live in `safe-cex-registry`"; P4 exit,
"a tier-1 account on the Rust path behind a flag").

TxFlow (`txflow`) is also constructible as its existing **Phase-0 scaffold**.
Its `start` emits and returns the exact source refusal before any IO; this entry
does not enable REST, WebSocket or trading support. `native_host_txflow` verifies
the production constructor and both network guards.

## Today

`loaders.rs` forwards a TypeScript module specifier and export name to a host
`NativeModuleHost`/`WebModuleHost`. That host returns an `ExchangeConstructor`.
The registry depends on no venue crate, so it constructs no Rust venue at all.

## Name to Rust constructor

A new module `rust_venues.rs` holds a fixed, ordered table of bound venues:
`ExchangeName -> fn(Rc<dyn RustVenueHost>) -> ExchangeConstructor`.

- **Initial members:**
  - `lighter` and `lighter-robinhood`, built with the existing `LighterNativeExchange::new`; the name selects deployment URLs and quote currency. The host retains the runtime transport and local signer entropy.
  - `grvt`, built with `GrvtExchange::new` over shared js_runtime; its host supplies fresh HTTP and the common external-public-data factory. The shared hosted signer is installed on its HTTP clients; active SDK order signing still requires a local wallet.
  - `asterdex`, built with `native_exchange::AsterDEXExchange::new` on the shared
    js_runtime. Its HTTP factory receives each lane's options, including secondary
    credentials and proxy override. The native host routes signed requests through
    the shared transport stack and supplies the external-data gateway factory.
    `tests/native_host_asterdex.rs` verifies a signed account read server-side.
  - `backpack`, built with `BackpackExchange::new`. Its native inputs retain
    the shared hosted HTTP signer and account name. Public requests bypass signing;
    private WebSocket auth continues to use the local secret as TypeScript does.
    `native_host_backpack_hosted` checks real HTTP and source failure/log behavior;
    `scripts/backpack-hosted-controls.py` proves signer, capability and log omissions.
  - `binance`, built with `BinanceExchange::new`.
  - `bitget`, `bitgetinverse` and `bitgetusdc`, one shared constructor around
    `BitgetExchange::new`; `BitgetDependencies` carries the runtime and a
    `JsHttpHost` built with js_runtime `rest_http_host`.
  - `bitrue`, built with `BitrueExchange::new` on the shared js_runtime; the
    native host supplies signed HTTP and the external-data gateway factory.
    `tests/native_host_bitrue.rs` independently verifies the request HMAC.
  - `bitunix`, built with `BitunixVenue::new` from a registry-built `BaseExchange`
    and `BitunixDependencies` (runtime, per-lane client builder and transport
    install, optional externalPublicData provider).
  - `blofin`, built with `BlofinNativeExchange::new` from a registry-built
    `BaseExchange` and `BlofinDependencies` (runtime, the common HTTP builder,
    optional externalPublicData provider, remote-signer account name).
  - `bybit`, `bybitdemo`, `bybitspot` and `bybitinverse`, built with `BybitExchange::create`. The host supplies
    the wire, HTTP host, runtime clocks and external-data factory. Hosted signing
    uses the shared signer bridge for both the source receiver and typed API
    clients; account names and availability follow the same capability. Native
    sockets start on the direct path. `tests/native_host_bybit.rs` verifies all four
    names over local HTTP, including HMAC bytes with empty local credentials.
  - `coinbase` and `coinbasespot`, one constructor around `CoinbaseExchange::new`.
    `CoinbaseDependencies` carries the shared HTTP/runtime hosts, signing cache,
    external-data factory and local-wall-time conversion. The native host retains
    its cache and routes hosted signing through the existing shared HTTP host.
    `tests/native_host_coinbase.rs` runs both names over local HTTP/WS servers and
    checks Coinbase Spot's source external-provider refusal.
  - `kodiakfi`, `mode`, `salsadex`, `tealstreet-dex`, `what` and `woofi`, built
    with `native_exchange::OrderlyExchange::new` on the shared js_runtime.
    `RustVenueHost::orderly(name)` supplies the runtime, per-options HTTP
    capability and external-public-data factory for the selected family name.
  - `okx`, `okxdemo`, `okxeu` and `okxeudemo`, one shared constructor around
    `OkxExchange::new`; the name picks the class. `OkxDependencies` carries the
    runtime and a `JsHttpHost`; the native host builds it with js_runtime
    `rest_http_host` over a fresh terminal adapter.
  - `binancepm` and `binancespot`, built with `BinancePMExchange::new` and
    `BinanceSpotExchange::new` on the same `RustVenueHost::binance` inputs.
- `hyperliquid` and `hyperliquidspot` call their respective production constructors with `RustVenueHost::hyperliquid(name, options)`: the host resolves the API origin through the selected transport, composes hosted HTTP signing and supplies signer metadata plus the shared weight limiter. Source Spot order signing retains its local-wallet requirement.
- `weex` uses `WeexExchange::new` with the shared signed HTTP/runtime hosts, gateway factory and native UTC date profile. `tests/native_host_weex.rs` verifies the actual HMAC at a local server; the native host takes the existing caller-owned Hyperliquid limiter.
- **Dependencies:** `safe-cex-registry` gains direct dependencies on those venue
  crates. The edges go downward (registry is above venues), but layering alone
  does not allow them: `layers.json` `registryConstructors` lists exactly the
  table's venues. `layers:check` rejects an unlisted registry -> venue edge and
  a listed venue with no live edge.
- **The returned type is unchanged.** Each entry returns the existing
  `ExchangeConstructor`. Its factory keeps the signature
  `(ExchangeName, ExchangeOptions, Rc<dyn Store>) -> Result<Rc<dyn Exchange>, ExchangeError>`.
  Venue host inputs come from one host trait, captured when the constructor is
  built:

  ```rust
  pub trait RustVenueHost {
      fn binance(&self) -> Result<binance::ExchangeDependencies, ExchangeError>;
      fn backpack(&self) -> Result<BackpackDependencies, ExchangeError>;
      fn okx(&self) -> Result<OkxDependencies, ExchangeError>;
  }
  ```

  - These are called once per construction, so every instance gets fresh
    runtime, signer, HTTP and transport inputs.
  - `BackpackDependencies` holds what `BackpackExchange::new` takes besides its
    `BaseExchange`: runtime, `BuildHttp`, `InstallTransport`, `DateParser`.
  - The registry builds that `BaseExchange` from the factory's name, options and
    store, so Backpack takes the same three caller inputs as Binance.
- **Failures:** a host or constructor failure is returned as `ExchangeError`.
  Nothing panics and nothing falls back silently.

## Adding a venue

When a venue lane finishes binding its class, it adds that venue in its own small
follow-up PR. The constructor table and its explicit membership test are the
current binding inventory.

Each follow-up does four things:
- adds one `RUST_VENUES` entry;
- adds its `RustVenueHost` method (with a dependencies struct if it needs one);
- adds the venue to `registryConstructors.venues` in `sdk-rs/layers.json` —
  `layers:check` rejects any registry -> venue edge that is not listed;
- adds the literal to the membership test, plus a construction test that calls
  one trait method;
- maps the new items as split members on that name's `exchangeRegistry.<name>`
  row.

## What still goes through the module host, and until when

- **Every web (wasm) entry.** Browser hosts keep importing the TypeScript build.
  The Rust path is native-only until the facade's wasm build links venue crates.
  That needs its own wasm size and feature audit, which this change does not
  claim.
- **Every name not in the table.**
  - A name moves to the table when its venue lane reports the class bound:
    all 16 trait methods live, or TS-identical refusals cited.
  - The table and the binding audit must agree. A test pins the table's members
    against a literal list, so additions are deliberate.
- **The module-host loaders are not deleted or changed.** The 94 loader entries
  and their CHOSEN rationale stay as they are.

## Selecting the Rust path (P4 flag)

```rust
pub enum ConstructionPath { ModuleHost, Rust }
pub fn native_constructor(
    name: ExchangeName,
    path: ConstructionPath,
    modules: &dyn NativeModuleHost,
    rust: Rc<dyn RustVenueHost>,
) -> NativeConstructorResult;
```

- **Caller-supplied, per name.** The caller (for example `exchange-server`)
  chooses the path for each name, from its own flag such as an allow-list of
  account or venue names. The registry reads no environment variable or global.
- **Default is `ModuleHost`.** With no flag set, behaviour is identical to today.
- **`Rust` for an unbound name is an error**, naming the venue (`no Rust
  constructor for <name>`). It never falls back to the module host, because a
  flag that silently lands on TypeScript would make the P4 exit claim
  unfalsifiable.
- **Same contract as the module-host path.** `Rust` returns the same `Ready`
  native result shape as the module-host path, so callers branch only on the
  flag.

## MANIFEST-SPEC anchors and rows affected

This changes the scope wording of one CHOSEN resolution, but none of its choices.

- **`#registry-loader-choice`:**
  - *Unchanged:* all three choices stay CHOSEN:
    - typed constructor capability instead of a JS export carrier;
    - fixed compiled tables instead of a mutable registry object;
    - caller-owned futures.
  - *Unchanged:* Rust constructors fit the existing `ExchangeConstructor` type.
  - *Changed:* the closing scope sentence no longer holds for the bound names.
    It currently reads "not proof that concrete venue module hosts,
    constructors or facade assembly have been supplied". It needs one added
    sentence: concrete Rust constructors exist for the names in `rust_venues.rs`,
    selected only by `ConstructionPath::Rust`.
  - *Owner:* this is an overseer edit to MANIFEST-SPEC.
- **`REGISTRY_CHOICE_RESOLUTION.md`:** "Typed constructor capabilities must not be
  reported as completed static venue linking" becomes true except for the
  table's names, which are statically linked on the native Rust path. This needs
  a matching addendum.
- **Rows `exchangeRegistry.backpack` and `exchangeRegistry.binance`**
  (`registry.native.ts.toml`):
  - They stay `divergent`, because the default path is still the module host.
  - Each gains its table entry as a named split member.
  - The new public items (`RustVenueHost`, `ConstructionPath`,
    `native_constructor`, `BackpackDependencies`) map onto those rows as
    split members, never as exemptions (MANIFEST-SPEC §5).
- **No other anchor changes.** `#owned-exchange-lease-policy` already covers
  `Rc<dyn Exchange>` handles, and `#exchange-runtime-callback-choice` already
  covers the runtime the host supplies.

## Proof

`safe-cex-registry/tests/rust_venues.rs`:

- **Construction through the registry:**
  - It constructs `backpack` and `binance` through
    `native_constructor(.., ConstructionPath::Rust, ..)`, using scripted
    runtime and transport hosts.
  - It asserts each instance's `Exchange::base().name` and capability table.
  - It calls one trait method end to end on each instance.
- **Error and default paths:**
  - `Rust` for an unbound name returns the named error, and the module host is
    never called.
  - `ModuleHost` still calls the module host.
- **Prove-fail:** swapping the two table factories must fail the name
  assertion.
- **Also run:** `yarn layers:check` and `parity:check` (staying at zero).

The native Kraken host shares its Futures nonce counter and Spot client-ID generator across instances constructed by that host. The generator uses OS entropy. Its typed constructor visibly refuses hosted `remoteSigner`; external public data uses the shared gateway factory. Journal folding remains an optional host dependency, not a newly implemented journal engine.

BingX uses `BingxExchange::new` with shared-js_runtime HTTP/socket dependencies.
Its native-host E2E test verifies signed startup/placement/cancellation; the venue
crate proves all 16 Exchange methods, retaining source-inherited rejections.
The native host also supplies its shared hosted signer to both HTTP and
BaseExchange. Empty-secret startup/placement/cancellation and signer-refusal
proofs run in native_host_bingx; the API skips local signing whenever that
capability is present, including when local credentials remain configured.

Orderly-family Rust construction covers kodiakfi, mode, salsadex, tealstreet-dex,
what and woofi. `RustVenueHost::orderly(name)` supplies their shared runtime,
per-options HTTP capability and external-public-data factory. The native host
retains the selected family name while routing the final signed request through
the shared transport stack. `tests/native_host_orderly.rs` verifies the actual
Ed25519 signature server-side for every family entry. The shared hosted signer
and account name now reach each per-options HTTP client and typed availability.
TypeScript still signs private HTTP locally before its hosted adapter, and its
private WS never reads remoteSigner. Thus local secrets remain required for
private HTTP and WS, while public HTTP still uses the hosted adapter. Empty
secrets preserve the source private HTTP error; private WS enters the already
approved shared onOpen recovery. `native_host_orderly_hosted` compares these
valid/empty-secret cases for all six names with actual source.

## Hyperliquid native host

Every native host constructor requires the caller's `Rc<HyperliquidRateLimiter>`.
Call `native_host::hyperliquid_rate_limiter()` once per LocalSet, then pass clones
to every direct and hosted account. Creating a limiter per account multiplies the
1200/60000 weight budget. `NativeAccountSpec` requires the same input; the FFI
command loop owns one limiter across all accounts and retains it across closes.

The native Hyperliquid host builds safeCexAxiosCreate with the venue's mainnet or
testnet baseURL and repeat query serializer, then installs the selected adapter
for private clients. A small adapter uses that resolved client default instead
of allowing ApiClient's per-request source baseURL to overwrite a proxy origin.
Hosted signers receive unsigned trading actions and their account name; `/info`
requests retain the skip-signing tag. The same hosted signer reaches native
BaseExchange capability and availability waiting. Transport/remote signing is
separate from the venue's local-key signing interceptor.

`tests/native_host_hyperliquid.rs` constructs through the registry, invokes real
trait methods over local reqwest HTTP, verifies signed action/nonce bytes on the
server, and checks both network origins, proxy preservation, info bypass, repeat
query encoding and limiter identity across separate account hosts.

## Bybit registry proof

`tests/native_host_bybit.rs` constructs Bybit and Demo through the production
native host and calls `get_account` over loopback HTTP. Each name is exercised
with local credentials and with a hosted signer plus account name and empty local
credentials. The server independently verifies the timestamp, API key, receive
window and generated `requestId` query bytes against HMAC-SHA256. Dropping either
table entry makes construction fail; dropping typed signer forwarding makes the
secretless test fail. All three controls were compiled, observed failing, and
restored before the passing two-test run. The existing 13 registry membership/
construction tests and 20 Bybit binding tests also pass (one live oracle ignored).

The untouched-origin witness records the terminal request before any URL rewrite.
It proves Bybit mainnet selects `api.bybit.com`, Demo mainnet selects
`api-demo.bybit.com`, and both testnet variants select `api-testnet.bybit.com`.
The old constructor failed specifically on Demo mainnet. Demo's source receiver
and socket connection options derive the same subclass flag by name.

KuCoin futures, KuCoin Spot and MEXC use their existing concrete venue bindings.
The native host supplies KuCoin's per-options HTTP inputs, shared virtual clock
and runtime-backed retry clock. MEXC receives its existing configured API stack
and native sockets; private fills forward to the constructed venue's emitter
through a weak reference. Native date conversion retains the shared UTC-host
assumption. KuCoin futures and Spot share the hosted account with both HTTP
clients, source options and typed availability through the common signer bridge.
Private WS obtains its token through that signed HTTP path. MEXC shares the
hosted account between configured HTTP, typed availability and private WS login,
including the socket account name. Public HTTP retains its source signing bypass.
Signer errors and absent WS signatures retain the source error message and send
no login or private filters. Its native HTTP/WS witness is
`tests/native_host_mexc_hosted.rs`. `tests/native_host_kucoin_mexc.rs` constructs
all three entries and verifies actual signed account reads server-side, including
KuCoin's signed passphrase and MEXC's stable account identity.
`native_host_kucoin_hosted` additionally compares actual TypeScript request
metadata, callback headers, unlimited cancellation and private subscription
topics over native local HTTP/WS with no local secret or passphrase.


KuCoin/MEXC construction validation measured fourteen scoped tests, zero parity
findings, live-source registry equivalence and all 88 layer assertions. Strict
host all-target and wasm library clippy passed. Independently removing each of
the three production table entries compiled and failed its native construction
test with exit 101; exact source restoration passed both native test functions.

Hyperliquid Spot is independently constructed as `Hyperliquidspot` through
`hyperliquidspot_constructor`, using the Spot production class and the shared
Hyperliquid dependency method. That method takes the selected name so native
transport and hosted-signing metadata retain the correct registry name. Both
names require the same caller-owned limiter and expose the configured public-data
factory. `native_host_hyperliquidspot` verifies native local HTTP/WS startup,
real action signatures, the caller cloid, shifted spot asset IDs and cancellation;
`native_host_external_data` separately verifies gateway startup.

ApexOmni is assembled through its shared js_runtime binding with a real
OmniKey-seeded zkLink contract signer. Its native proof observes HTTP/WS startup,
wire HMAC over the transmitted order form, caller ID, private WS order delivery
and cancellation. Unsupported hosted signing/wallet-provider operations reject
visibly. The module-host path remains the default.

Spot uses the same explicit native host and existing registry layer edge to the
Bybit crate, with its distinct `BybitSpotExchange` receiver. The Bybit native
HTTP tests include Spot for local HMAC and secretless hosted signing. Its
category/market/leverage, order writes and order-only private subscriptions are
covered by `safe-cex-venue-bybit/tests/spot_binding.rs`. Inverse has its distinct
receiver; HyroTrader now also selects its simulator receiver and acknowledged
private session through the same Bybit host edge. Missing build-time origins
retain the source configuration refusal. `native_host_hyrotrader` verifies real
native registry construction and signed HTTP reads.

Crypto.com is in the Rust constructor table. `tests/native_host_cryptocom.rs`
constructs the bound venue using production native inputs and the shared host
limiter argument, then checks its identity, distinct market precision steps,
real local HTTP dispatch and Store read purity. The registry passes through the
account's hosted signer and external-public-data provider to the venue binding.

Phemex constructs through `phemex_constructor` and the production shared Js HTTP
host, including hosted signing and the external-public-data gateway. Native host
callers supply the shared limiter as for other venues. For `tealstreetTunnel`,
the native host installs the actual provider-selected default adapter through the
shared Js HTTP factory. `tests/native_host_phemex.rs` verifies real local HTTP
market precision/read purity and signed account-query bytes through the registry.

`tests/native_host_tunnel.rs` also reads Phemex public products and a signed
account query over an authenticated local tradetunnel, with no direct HTTP.
The independent query HMAC, distinct precision steps and default frame format
are checked. `scripts/phemex-tunnel-controls.py` proves missing/no-op native
provider mutations compile and fail this witness, then restores exact bytes.


AsterDEX native hosts now share their hosted account with the source receiver
and signed HTTP clients through the common signer bridge, retaining per-lane
post-sign routing. V1 and V3 account reads and validation work without local
secrets; the source's nonempty API-key rule remains. Its raw clock HTTP bypasses
hosted signing. `native_host_asterdex` covers metadata, exact returned wire
configuration and the key refusal; its four compiled controls live in
`scripts/asterdex-hosted-controls.py`.

Paradex's Rust entry calls `native_exchange::ParadexExchange::new` with shared
runtime HTTP and external-data hosts. `native_host_paradex` constructs the
production registry route over local HTTP and checks native amount/price steps,
venue identity and source fetchMarkets Store purity. The native host explicitly
refuses remoteSigner until the SDK account capability is bound; it never falls
back to local credentials for a hosted account.

Breakout Prop is the legacy DXtrade adapter, assembled through its existing
constructor and ProductionHost rather than another venue's implementation.
All 16 trait methods dispatch on shared js_runtime, including seven exact
inherited source rejections. Native HTTP preserves DXAPI headers and the
transport-resolved baseURL; tunnel installation follows the source hook.
Hosted login refuses visibly. Native offset-less dates use an explicit UTC
realm; embeddings with other timezone policies supply local_to_utc.
