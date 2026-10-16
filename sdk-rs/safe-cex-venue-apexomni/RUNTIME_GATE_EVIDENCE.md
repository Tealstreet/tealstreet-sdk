# ApeX Omni runtime slice

This slice moves **30** previously pending rows: **3 ported, 27 divergent**.
The independent baseline is `b6b9db662763d63c1f2a960992129bc23107d264`:
24 ported / 75 divergent / 69 pending. This slice leaves
27 ported / 102 divergent / 39 pending, of 168 rows. Counts use Python 3.11
`tomllib`, comparing each manifest symbol by `(ts, kind)`.

Implementation commit: `7074530c94`. Integration merge:
`590624a97edb4b9c585aed9576ac4492feebb80e`, with second parent
`17b6bcd0db760cfb175d1f5e21e581b5d29bd2a5`. No unreviewed lane work was
inherited at the baseline. The exemption conflict retained both lanes' entries.

## Bucket justification for these 30 rows

These classifications follow the implemented boundary of each body and its
native comparison, independently of any previous ApeX Omni slice.
The string boundary is defined once at
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary`.

| Row | Bucket | Reason |
| --- | --- | --- |
| `hashClientOrderId` | ported | Accepts all UTF-16 code units, including lone surrogates. Replacement before SHA-256 matches TextEncoder. Native digest bytes are compared with actual noble source calls. |
| `hmacSha256Base64` | ported | The same full UTF-16 input domain covers both message and key; standard base64 output is compared directly with actual source HMAC calls. |
| `ApexOmniExchange.get accountType` | ported | Binary64 state comparison with 1 and the two ASCII literal results have no narrowed value or host boundary; cases include NaN. |
| `feeHelper` | CHOSEN | Host callback replaces a dynamically selected mutable Math property. Keeping that capability at the host avoids importing a JavaScript global object into the venue. Exact decimal multiplication and binary64 toFixed remain in the body. |
| `serializeApexPayload` | CHOSEN | Owned prepared values and pinned root collation replace executable getters, Date instances, cycles and ambient locale state. A second JavaScript heap would detach signed parameters from shared transport. Falsy values are retained. |
| `serializeApexPayloadLegacy` | CHOSEN | Same prepared-value and collation boundary; this body independently preserves legacy recursive removal of falsy values. |
| `initSdk` | CHOSEN | A shared module initialization state uses host-polled futures rather than eager JavaScript Promise scheduling. Keeping scheduling in the host avoids a second executor. The flag changes only after success and does not coalesce in-flight calls. |
| `signOrder` | CHOSEN | Typed payload and opaque host builder/signer capabilities replace arbitrary JS objects and imported wallet/WASM globals. Validation and ordered builder/sign calls are in Rust; independent host cryptography is not claimed. |
| `signTransfer` | CHOSEN | The same wallet ownership boundary preserves amount/fee packing, timestamp fallback, validation and transfer builder/new/sign ordering; futures require host polling. |
| `signWithdraw` | CHOSEN | The same boundary preserves dust truncation before packing, timestamp fallback, validation and withdrawal builder/new/sign ordering. |
| `signChangePubKey` | CHOSEN | The same boundary preserves public-key fallback, packing, builder arguments, signature literal, authentication selection and unsupported CREATE2 error. |
| `discardBeyondPrecision` | CHOSEN | Native Result retains ethers error messages rather than JS Error identity, stack and mutable properties. Using shared error plumbing avoids a separate error heap. Integer truncation and guard ordering remain in the body. |
| `ApexOmniExchange` | CHOSEN | Shared BaseExchange/Store and explicit HTTP/socket host capabilities replace JS class objects and ambient factories. Native model types visibly reject unsupported required values; retaining a JS heap would split this venue from shared SDK ownership. |
| `ApexOmniExchange.constructor` | CHOSEN | Receives a constructed shared BaseExchange and host factories, then installs the actual rule/state and creates rate-limited/unlimited clients and both sockets in source order. Arbitrary subclass/prototype construction is outside this ABI. |
| `ApexOmniExchange.dispose` | CHOSEN | Composes shared BaseExchange disposal with two host socket disposals. Host errors use Result rather than arbitrary JS throws; a duplicate venue disposal system would bypass shared lifecycle ownership. |
| `ApexOmniExchange.getAccount` | CHOSEN | Returns owned prepared field values rather than aliases/getter evaluation on the response object. Keeps the shared HTTP response model and explicit null affiliate field. |
| `ApexOmniExchange.fetchRawBalance` | CHOSEN | Shared typed Balance and retained info replace unrestricted JS records; native NaN metadata is refused visibly. Avoids a second balance/info model while preserving missing-payload fallback and numeric formulas. |
| `ApexOmniExchange.fetchBalance` | CHOSEN | Returns owned typed Balance and composes the shared disposal state. Eager Promise scheduling and mutable result aliases are outside the native ABI. |
| `ApexOmniExchange.fetchPositions` | CHOSEN | Shared typed Position/Store records and retained configuration replace mutable JS records. Keeps fake-side creation, first-ID deduplication and account configuration effects in the body. |
| `ApexOmniExchange.fetchOrders` | CHOSEN | Shared typed Order outputs and polled HTTP future replace unrestricted records and eager Promise scheduling. Preserves the 10-second blacklist cleanup and mapping order. |
| `ApexOmniExchange#normalizeApexSymbolKey` | CHOSEN | Shared UTF-8/prepared-value boundary replaces arbitrary JS coercion objects. Keeps ASCII filtering and prefix/suffix operation order instead of creating another SDK string model. |
| `ApexOmniExchange#normalizeExecutionTimestampToSeconds` | CHOSEN | Host Date.parse capability and prepared-value coercion replace ambient Date/global objects. Host errors propagate unchanged; owning date parsing in the runtime avoids a second date environment. |
| `ApexOmniExchange.recordWsExecution` | CHOSEN | Shared typed Execution fields and owned cache entries replace JS property aliases. Keeps field precedence, normalization, deduplication, timestamp sorting and the 500-row limit. |
| `ApexOmniExchange#getWsExecutionFallback` | CHOSEN | Shared typed Execution options/results replace unrestricted JS objects and aliases. Keeps normalized symbol matching, filters and JS slice limit behavior in the body. |
| `ApexOmniExchange.fetchMarkets` | CHOSEN | Composes the native typed helper, whose Vec cannot contain null or be monkeypatched to return null. Arbitrary JS override results are deliberately outside the shared Market ABI. |
| `ApexOmniExchange.fetchMarketsHelper` | CHOSEN | Shared typed Market values reject non-string required fields and non-boolean active state visibly. Keeping one market model is preferable to carrying arbitrary JS record values through Store. Configuration update still precedes mapping. |
| `ApexOmniExchange.cancelOrders` | CHOSEN | Takes shared typed orders and polls the sequential helper future. This retains cancellation behavior while leaving Promise scheduling with the host. |
| `ApexOmniExchange.cancelOrdersHelper` | CHOSEN | Typed orders and host requests replace arbitrary records and Axios objects. Keeps blacklist-before-network, local removal, 10-item chunks, inter-chunk sleep and source duplicate-base cancellation behavior. |
| `ApexOmniExchange.mapPosition` | CHOSEN | Shared typed Position and retained info replace mutable JS records; native NaN metadata is refused. Source alias lookup, leverage fallbacks, side defaults and unknown-market warning remain in the body. |
| `ApexOmniExchange.mapOrder` | CHOSEN | Shared enums, boolean reduceOnly and string IDs reject unsupported JS values visibly. Avoids fabricated enum/field defaults while preserving trigger lookup quirks, leg ordering and bracket IDs. |

The already divergent `assertAddress` and `signContract` rows also acquire
complete venue bodies in this slice. They are not included in the 30-row count.
Their CHOSEN typed validator/wallet boundaries remain explicit; the generic
cryptographic primitive is no longer presented as the complete signContract body.

## Native comparisons and discrimination

`tests/pure.rs` compares native digest bytes, strings and Result errors for
12 hash strings, 144 HMAC pairs, 13 serializer cases per serializer,
100 precision cases and 180 fee cases. `tests/signing.rs` compares native
results and ordered capability arguments for six cases per transaction body,
plus serial SDK initialization. `tests/exchange.rs` compares native model
fields, enum tags, cache/configuration state and operation order for 54 cases,
three per method. Typed model comparisons do not claim JS object insertion order.

Before the merge, the compiled pure controls failed the six named native
assertions with exit 101; the SDK flag control separately failed its named
operation assertion with exit 101. Restoration passed all 12 pure/signing tests.
Five signing-builder controls failed the named native operation assertions,
then restoration passed six signing tests. The exchange leaf and caller
groups failed their named native assertions with exit 101, then restoration
passed all 18 exchange tests. No surviving control was omitted.

The post-merge command
`git diff --exit-code 7074530c94 HEAD -- sdk-rs/safe-cex-venue-apexomni/src sdk-rs/safe-cex-venue-apexomni/tests sdk-rs/safe-cex-venue-apexomni/scripts`
returned **0** at `590624a97e`: production bodies, native assertions and oracle/control
inputs are byte-unchanged. Controls were not rerun after this merge.

## Gate scope

Workspace compilation, full suites, host clippy, wasm clippy, live HTTP/WS
execution and independent host WASM cryptography were not run or claimed.

Measured at `638af43ac47a30a0f3e142a1c111f31691c7e554`, after merging
`df065d29b58fd12bdc103e2ef156c8de602663a0` down:

| Command | Measured exit / result |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-apexomni --all-targets --jobs 2` | 0 |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-apexomni --jobs 2 --test pure --test signing --test exchange -- --test-threads=1` | 0; 6 pure + 6 signing + 18 exchange tests passed |
| Compiled `is_perpetual: Some(true)` control, scoped to `markets_helper_native` | 101; failed `Apex market omits isPerpetual` |
| Restoration, followed by the same three scoped test targets | 0; all 30 tests passed |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; would update 0 manifests, 0 added, 0 orphaned |
| `git diff --exit-code 772b81a2ae HEAD -- sdk-rs/safe-cex-venue-apexomni/src sdk-rs/safe-cex-venue-apexomni/tests sdk-rs/safe-cex-venue-apexomni/scripts` | 0; latest integration merge left these inputs byte-unchanged |

Oracle invocations used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
The independently stored command/exit sheet is
`sdk-rs/target/apexomni-controls/merge-gates.json`; logs remain ignored.

The first post-17b6bcd0db test command failed compilation with exit 101 because
integration added the optional shared Market `is_perpetual` field. The next
attempt failed compilation with exit 101 because integration also added the
required `ExchangeRuntime::monotonic_now_ms` capability. Commits `d72fc665b9`
and `772b81a2ae` adapt the constructor and test host, respectively. The former
sets None because the actual ApeX source omits isPerpetual; the latter supplies
a deterministic independent monotonic clock and asserts the omitted native field.
The successful measured gates above cover both changes.
