# Registry data

The behavioral reference is [the exchange guidance](../../packages/safe-cex/src/exchanges/CLAUDE.md).

`registry` holds ordered descriptors from both TS registries; `loaders` holds
47 executable web/native pairs backed by the explicit `module_host` contracts.
Descriptors support inspection without invoking hosts. Constructor capabilities
call supplied factories; concrete venue host/facade assembly is separate.
The SDK facade is one layer above registry.

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
order; the 47 entry functions in each registry have their own source/native witnesses.

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
