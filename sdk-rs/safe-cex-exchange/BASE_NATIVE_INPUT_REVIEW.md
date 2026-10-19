# BaseExchange native input owners

This cohort classifies seven pending `js_runtime::base_exchange` owners. Every
owner has its exact compiler function API and an executed adapter, rather than a
role-only classification. The overlay callback is now fallible; its former report-and-return fallback is removed. The native
host-dependent probes explicitly require the `native` feature; their default
receipt remains visibly native-gated until the canonical native probe executes.

| Owner | Boundary, native probe and ruling |
| --- | --- |
| `create` | ACCEPT: composition of the typed base and raw receiver. Construction reads source flags before publishing the base, retaining the original options/store referents. Direct receipts check both flag combinations and identity. The installed native facade and existing source-identity tests cover actual initialization and original getter failures. This does not grant decoding permission to callbacks installed by the factory. |
| `create_xhr_with_secondary_key` | ACCEPT: structural typed options select the existing transport split; the callback receives the original primary options and a raw secondary spread. Direct and installed-source probes cover one/two clients, secondary overrides, executable signer capabilities, unread lone UTF-16 units, and the original callback failure identity. Neither callback inputs nor returned Axios handles pass through JSON. |
| `exchange_options` | ACCEPT: explicit construction-time projection into `ExchangeOptions`, excluding executable remote/payload signer handles. Direct native receipts preserve those original capabilities and unread raw UTF-16 on the original object while observing typed key/secret fields. This is the existing typed configuration boundary, not a claim that typed options retain arbitrary unknown properties. |
| `market_overlay` | FIX: nullish input means no overlay; otherwise the retained callable is invoked when typed markets are adapted. Direct receipts cover null, undefined, identity and throwing callbacks. Installed native/source comparisons cover loading and invocation. The direct fallible bridge propagates callback/conversion failure instead of returning unadapted markets. The compiled regression preserves the original thrown referent, name and UTF-16 message. Source loadAccountMarketOverlay still owns its separate catch when applying a newly loaded overlay to an already populated Store; this change preserves that catch. |
| `reads_on` | ACCEPT: native capability adapter with a prototype link to the primary and a secondary `get` closure. Actual primary/secondary callback calls preserve payload references, own undefined, NaN and UTF-16 tag units. The installed comparison executes the maintained Proxy implementation with identical client callbacks, including the ordinary `post` receiver. |
| `text` | ACCEPT: shared string-hint conversion followed by the existing Rust `String` materialization. Executed inputs include undefined, null, number, text, lone surrogate and an original coercion failure. Valid units match JS; the typed String boundary materializes an unpaired unit as U+FFFD, as the established typed-string representation does. The installed `getOrFetchPositionsForSymbol` rejection preserves the actual original thrown referent. No raw UTF-16 retention is claimed for this Rust String result. |
| `worker_rpc` | ACCEPT: delegates to the existing core JSON worker-request boundary, owning no parallel request protocol. Direct receipts cover dead context and live-but-unassembled context rejection. The actual attached native facade covers eager request emission, method/argument fields, successful result and reconstructed failure name/message/code/stack; the pre-existing recording-host test also covers completion hooks and timeout cleanup. Raw callbacks and arbitrary non-JSON values are not admitted by this existing wire boundary. |

`tests/js_async_facade.rs` constructs the actual `ScopeShared`, installed globals,
canonical Store, native HTTP host and `BaseExchange` facade with `NativeRuntime`.
`tests/js_async_facade_source.cjs` executes the unchanged maintained BaseExchange
members and `createApiWithSecondaryKey` implementation extracted with TypeScript's
parser. Native observations inspect references, units and fields before any JSON
observation output. Existing `tests/js_runtime.rs` constructor, client routing and
worker completion tests remain intact. These are bounded observations, not proof
of all JavaScript programs or an authorization to weaken any decoder policy.
