# Rust venue construction through the registry

Status: implemented for `backpack`, `bitunix`, `blofin`, the Kraken family (`kraken`, `krakenspot`), the Binance family (`binance`, `binancepm`,
`binancespot`) and the OKX family (`okx`, `okxdemo`,
`okxeu`, `okxeudemo`), 2026-10-16 (`src/rust_venues.rs`,
`tests/rust_venues.rs`). Plan reference: `docs/plans/SAFE_CEX_RUST_PORT_v1.md`
(crate layout, "Constructors and loaders live in `safe-cex-registry`"; P4 exit,
"a tier-1 account on the Rust path behind a flag").

## Today

`loaders.rs` forwards a TypeScript module specifier and export name to a host
`NativeModuleHost`/`WebModuleHost`. That host returns an `ExchangeConstructor`.
The registry depends on no venue crate, so it constructs no Rust venue at all.

## Name to Rust constructor

A new module `rust_venues.rs` holds a fixed, ordered table of bound venues:
`ExchangeName -> fn(Rc<dyn RustVenueHost>) -> ExchangeConstructor`.

- **Initial members:**
  - `backpack`, built with `BackpackExchange::new`.
  - `binance`, built with `BinanceExchange::new`.
  - `bitunix`, built with `BitunixVenue::new` from a registry-built `BaseExchange`
    and `BitunixDependencies` (runtime, per-lane client builder and transport
    install, optional externalPublicData provider).
  - `blofin`, built with `BlofinNativeExchange::new` from a registry-built
    `BaseExchange` and `BlofinDependencies` (runtime, the common HTTP builder,
    optional externalPublicData provider, remote-signer account name).
  - `okx`, `okxdemo`, `okxeu` and `okxeudemo`, one shared constructor around
    `OkxExchange::new`; the name picks the class. `OkxDependencies` carries the
    runtime and a `JsHttpHost`; the native host builds it with js_runtime
    `rest_http_host` over a fresh terminal adapter.
  - `binancepm` and `binancespot`, built with `BinancePMExchange::new` and
    `BinanceSpotExchange::new` on the same `RustVenueHost::binance` inputs.
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
follow-up PR. Expected next: `bitget`, `bybit`, `kucoin`, `mexc`, `lighter`
and `woo`.

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
