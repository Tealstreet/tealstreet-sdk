# Bitrue public aliases

After the 32 receiver reviews, compiler discovery exposes exactly seven public
aliases. The baseline runner authenticates fresh rustdoc, actual compiler
artifacts/sysroot identities, and each source declaration's existing manifest
claim. All seven initially fail their missing contracts by name.

| Alias | Role and bounded proof |
| --- | --- |
| `wire::FetchContractsResponse` | Actual Vec decoder. Native observation reads all fourteen fields on two distinct rows, including UTF-16 string units and every binary64 bit pattern, without Serialize. Empty and malformed lists are separate cases. |
| `wire::FetchKlineResponse` | Actual Vec decoder. Native observation reads every string's units and timestamp bits on distinct rows, plus empty/malformed cases. |
| `position_metadata::PositionMetadataHandle` | Cache-owned Rc/RefCell capability; its underlying metadata record has no raw decoder. Actual constructed parent fetchPositionMetadata populates its native cache; the typed cache-hit consumer checks duplicate/cache referents, every field and shared mutation. |
| `public_bodies::NativeCallback` | Callable capability. Canonical generic probe is the existing fixed Value/Value instantiation, not an unbounded V/E claim. Actual constructed-exchange consumer also exercises Js/Thrown arguments, return identity and original rejection identity. |
| `collection_reads::Callback` | Callable capability fixed to the actual production `binding::Rt` (Js/Thrown). The real host function consumes native callbacks with argument order, signed-zero and original resolution/rejection identity checks. |
| `trading_rest::Body` | Native pinned future fixed to the same production Rt. Actual host/Promise consumption checks pending work, wake/release, and original resolution/rejection identity. |
| `trading_rest::AsyncCallback` | Executable callback producing that Body. The real `TradingHost::async_function` is the consumer; callback invocation count, argument bits and Promise results are observed. |

Three exact `generic-aliases` records register only the production Rt
monomorphizations and expected compiler types. The maintained engine still owns
type equality, actual Deserialize/alternate trait detection and artifact checks.
No identity fallback, generic substitution family, receiver guard, or shared
probe algorithm changes. Metadata/string/numeric admission keeps the existing
wire boundaries; these finite observations do not claim arbitrary JavaScript
input parity or whole-venue accepted-set completion.

The native fixture is a child of the receiver fixture in `tests/support`, so it
uses the actual `BitrueExchange::new` and original Scope, Base, Store, globals and
runtime. Unexpected HTTP panics. The pending input future registers and wakes
its real task waker; ready work is pumped without advancing time or sleeping.

Run alone from `sdk-rs`, with one Cargo process:

```sh
RUSTC_BOOTSTRAP=1 node safe-cex-venue-bitrue/scripts/public-alias-contracts.mjs
cargo test --locked --jobs 1 -p safe-cex-venue-bitrue --lib public_aliases
python3 safe-cex-venue-bitrue/scripts/public-alias-controls.py
```

Rustdoc remains in the helper's separate accepted-inventory target; the native
probes use the maintained accepted-set build. The control script must restore
original bytes after every independent production fault before another build.
A STOP file in `target/bitrue-public-alias-controls` stops only between restored
cases, allowing the full Store step-4 merge to release the array priority.

All seven independent production controls pass: both synchronous callback
returns, asynchronous argument order, future result retention, native metadata
handle identity, and same-type field swaps in each actual response-list decoder.
Every fault compiled and failed its named observation; exact source restoration
passed and all source hashes match. The full canonical inventory now reaches the
separate `binding/classes.rs::UNSUPPORTED_LOCAL_MACRO_EFFECTS:enums` boundary.
That census issue is a follow-up, not a full-package GREEN claim here.
