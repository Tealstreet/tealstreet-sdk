# Transport

The behavioral reference is [safe-cex/CLAUDE.md](../../packages/safe-cex/CLAUDE.md)
-> Proxy & Tunnel Architecture, and the root rule on non-browser tradetunnel
runtimes. Representation decisions are DECLARED once in
[`../parity/MANIFEST-SPEC.md`](../parity/MANIFEST-SPEC.md) and cited by anchor
from rows; do not restate them here or in a row note.

**`serde_json` is pinned `=1.0.152` with `preserve_order`, `float_roundtrip` and
`arbitrary_precision`, and `how.rs` ASSERTS the sdk workspace dependency is
byte-identical to `infra/tradetunnel/Cargo.toml`'s.** `frames.rs` is shared by
path between the two trees, so a `Map`/`Number` semantics difference would be a
silent wire divergence rather than a compile error. The assertion reads the
**workspace** dependency entry, so a member crate may add a capability feature on
its own dependency line without tripping it — `safe-cex-types` already carries
`features = ["raw_value"]` that way, which is the path for the lexeme-preserving
numeric ingress (a feature on a crate already in the graph, not a new
dependency). Changing the shared entry itself needs the identical change in
`infra/tradetunnel/Cargo.toml` in the same commit.

**Retry is a type-state, and the token is the contract.** A `ReadRequest` cannot
be built from a write, a `RetryPolicy` is not a `RestAdapter`, and
`RetryPolicy::execute` takes a `RateLimitClock` because the remaining-time budget
is DERIVED inside the engine from measured attempt time — never supplied per
attempt and never read from `error.config`. A retried error's own config is
untrusted: it is re-classified through the ORIGINAL policy's
`ReadClassification`, so an adapter cannot talk a write into the read lane.

**Six `compile_fail` doctests guard those boundaries and there is no `trybuild`
dependency.** `compile_fail` proves the example does not compile; it does not pin
WHICH diagnostic, and error codes in doctests are inert on the pinned cargo. If a
guard needs to prove a specific message, it needs trybuild stderr matching, not a
stricter doctest.

**`RestBody` deliberately has no `Default`** and the three variants are a
provenance, not a convenience: signing cannot recover String-versus-Object
identity from encoded bytes (binance signs a JSON-looking *string* by character
index), so a signing interceptor's data goes back through
`with_signed_data`, which keeps the transform phase. Content-type matching is
case-sensitive, as in Axios, and the successful-parse branch of the JSON string
path **trims** — parse happens before the trim because a BOM is valid JS trim
whitespace and invalid `JSON.parse` input.

**`how.rs` is where lexeme retention is already proven in production** ("number
to string erases -0; retain its lexeme until wire projection"). It is the
reference mechanism for the shared numeric ingress, not a local trick.

**`how_diagnostics::summarize_trade_tunnel_url` substitutes U+FFFD** at a split
surrogate truncation boundary, and its row currently claims `FORCED`. That
conflicts with the UTF-8 string boundary declaration on both counts; the conflict
is recorded, unresolved, in MANIFEST-SPEC §19 (`forced-surrogate-conflict`).
Do not "tidy" either side before that is ruled on.

Gates: `cargo test -p safe-cex-transport`, strict clippy on the host **and** on
the wasm target (every crate in `.github/rust-wasm.json` is mandatory there — the
host lint cannot see `cfg`-gated browser code), plus root `yarn routing:check`
for generated routing freshness and the golden resolver cases.

`RestResponseBody` has explicit Encoded, Untransformed and Transformed variants,
with no Default or implicit byte dereference. Ordinary HOW frames retain native
data; custom-parser results retain Transformed provenance. A subsequent source
parser receives CURRENT data even if a prior parser ran: Binance's quoted numeric
HOW response is deliberately parsed twice (ts-bug), unlike direct HTTP bytes.
`json_value` is a model-view projection, not a claim that a response transform
ran. Use `transform` or `try_transform` for a source parser, never serialize and
reparse native values to infer the phase. Parser errors remain caller-visible.
The native payload is detached from the cloned frame before typed envelope
metadata is decoded. Passing that payload through serde_json::from_value would
rebuffer lexical -0 into +0 before RestResponseBody even sees it. Keep the payload
outside that decoder; this is preservation of current adapter data, not a second
numeric parser or a typed-f64 ingress implementation.

**Remote signer JSON replay.** `try_parse_json` normalizes only a successfully
parsed object/array STRING using HOW's canonical numeric/own-key projection.
Finite numbers collapse to binary64 spelling before reaching a signing consumer;
negative zero and overflow retain their lexemes for shared String/Boolean/number
coercion. Existing non-string values remain untouched, matching TypeScript's
identity passthrough. The live-source corpus observes number bits, String and
truthiness BEFORE serialization, because JSON null would hide overflow loss.
The helper's inherited parse-refusal fallback matters: valid lone-surrogate JSON
and over-depth objects remain the original String where TypeScript returns an
object. Its manifest row declares these independently from numeric normalization;
`PARSER_NUMERIC_EVIDENCE.md` records the raw source witnesses and positive Unicode.

Signing envelope and secondary-options gates assert decoded native fields
before re-encoding; roundtrip bytes cannot detect symmetric serde-name swaps. Secondary
client derivation clones the typed options and overlays only the declared
secondary credentials plus the selected proxy. Never serialize account options
to copy them: JSON would erase native nonfinite signer types. The source oracle
checks nested option identity before fixture encoding, and the native regression
checks exact signer-type bits including negative zero and NaN payloads.
The closed tryParseJson bit corpus cannot produce NaN and refuses it at capture.
Its provenance supplement retains the old golden bytes, records the unknown
original architecture, and distinguishes fresh verification from original capture.
The staging oracle batches all environment flags in one Node process with a
fresh source VM per flag. Rust child processes remain isolated because native
endpoint initialization is process-global; each receives its source URL row.

The raw parser oracle pins ten source cases and executes a fixed ten-case native
walk with thirty comparisons. Source length, missing-index and compared-coverage
checks are independent: empty, shortened or surplus captures must fail even when
the first count guard is bypassed. Its permanent child controls execute one Node
process per corpus; `scripts/how-parser-cardinality-controls.py` additionally
proves the guards are load-bearing and restores the source after mutation.

The safe-cex HOW wrapper source witnesses live in
`scripts/safe-cex-how-wrappers.cjs`. They execute the two actual wrapper modules
with explicit dependency spies, not the full metadata/access-token services.
The package HOW manifests and safe-cex `httpOverWebsocket` manifests name distinct
source trees; pending wrapper dependencies must not be counted as package rows.

Query parameters use `QueryValue`, independently of JSON bodies. Each request
can name an indexed or repeat `QueryProfile`; factories may supply that profile
as a default before interceptors. Local signing/retry retain native numbers and
undefined. JSON projection is named and confined to remote protocol/diagnostic
boundaries; it is never used to render a query string. Wire decoding captures
numeric tokens with the canonical RetainedJsonInput before constructing query
variants, preserving numeric-looking strings. There is no Value conversion shim.

Accepted-set source discovery reviews local form/query output helpers and retry
predicates individually, with exact source pins. Their native witnesses are in
`tests/accepted_local_helpers.rs` and `tests/query_params.rs`: they observe output
bytes, depth boundaries, retained input identities and predicate error order.
The private `QueryWire` serializer stays at module scope so its generic Serialize
implementation remains visible to ordinary compiler discovery. Explicit imports
and the qualified wasm-bindgen attribute keep macro origins unambiguous; they do
not grant permission to unknown local macros or input decoders.

RestAdapter success receipts carry the returned response together with its actual
request configuration. The default receipt method is for terminal adapters;
rewriting middleware forwards execute_with_receipt and preserves inner changes
and omissions. Retry, remote signing and rate limiting retain that receipt.
HOW deliberately reports its original input configuration on success, matching
the source wrapper, including simulation; its failures retain inner provenance.

Region adapter selection retains JA/SG and passes other configured strings,
including empty strings, through Other to the host. The provider decides
whether to replace the adapter; the venue does not reject those strings first.

The selected transform_axios_response parser runs once on current adapter data.
Physical decoding removes the initial BOM before parsing; native strings do not.
It preserves binary64 number behavior and own-key ordering through the existing
HOW normalization. Valid unrepresentable JSON is distinguished from invalid
syntax under [the shared boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary).
Signature rejection predicates retain String substring versus Array exact
membership and typed JavaScript TypeError outcomes for non-callable includes.

HOW public message entry ignores direct post-disposal calls before decode/dispatch.
Plain form fallback preserves decoded-key primitive __proto__ setter omission;
valid JSON own properties remain intact. Behavior tests assert native effects first.

The shared socket-options carrier exposes optional `start_closed`; exchange-core
WS_CONTRACT.md owns its managed-wrapper lifecycle and remaining option gaps.

RestResponseBody::json_value captures encoded bytes through RetainedJsonInput
before constructing Value, preserving literal reserved-looking object keys.
This model read does not change the separately observable Axios transform.

**`native` (feature, non-wasm only) is the physical I/O under every codec.**
`ReqwestAdapter` sends exactly the bytes it is given and settles non-2xx as a
response-bearing failure, like axios `settle()`; leftover params use axios's
default encoder only when no factory codec consumed them. `connect_websocket`
queues events from connection and offers exactly `http/1.1` ALPN, which tradetunnel
requires (REST keeps the default). One TLS stack: rustls on the `ring` provider with
native roots; do not add a dependency that pulls aws-lc. Run inside a tokio
`LocalSet`. Proofs live in `safe-cex-registry/tests/native_host_*.rs`.

`RestRequest::config_extensions` uses the canonical `RetainedJsonInput` data
carrier separately from headers across terminal adapter calls, hosted signing,
retry, and effective response/error config. Own undefined and native f64 values
(including NaN payloads, infinities and signed zero) remain native. The shared JS
client uses its existing typed capture/restore codec; callbacks and ordinary
request fields keep their existing capabilities and typed paths. HOW alone
projects truthy `grvtCookie`/`grvtAccountId` extensions into Cookie/account headers,
matching the actual browser adapter. Direct HTTP keeps X-Ts-Grvt-Session source
semantics. Unknown extension properties are retained, not emitted as HTTP headers
or HOW arguments. HOW applies native truthiness first, then the shared
JSON.stringify projection at its actual frame boundary. This data-only typed profile retains its existing
BigInt/string, UTF-8, depth and owned-tree limitations; it does not carry arbitrary
executable Axios objects or live aliases. The live source corpus in
tests/grvt-config-source.cjs covers seven JSON cases plus five separately named
native-value cases. The hosted-signing receipt witness checks own undefined,
NaN payload bits and signed zero at the terminal adapter before any projection.

The native terminal combines baseURL and relative URLs using Axios
buildFullPath/combineURLs semantics: exactly one joining slash, empty relative
path retains baseURL, absolute/protocol-relative URLs remain absolute. The
native::url_tests regression was measured against actual installed Axios and
prove-failed on the old concatenation; DXtrade's /dxsca-web base exposed it.

Concrete frame/routing/header object admission and the region string unions are
reviewed in [ACCEPTED_SHAPES.md](ACCEPTED_SHAPES.md). The shared standalone frames
use owner-specific `deserialize_map` visitors and private remote schemas; SDK
records use the existing object guard. Keep generic borrowed request payloads,
all original field/presence rules and the independently inventoried visitor
contracts. The fourteen compiling disconnect controls restore exact source bytes.
The bounded follow-up API reviews are recorded in
[ACCEPTED_API_REVIEWS.md](ACCEPTED_API_REVIEWS.md). Its query and
diagnostic section records the direct retained-number field decoder, JSON
projection, error construction, raw URL parsing and separate typed selector
review, with native/Wasm receipts and serial mutation controls.

The body/secondary accepted review executes all eleven inventoried body APIs,
including current-data callbacks and separately observable model reads. Its
controls require compiling named output failures and exact restoration; run
`scripts/accepted-body-controls.py` serially with other source-sensitive probes.
The accompanying Axios oracle observes actual transforms and dispatch data.

Signature-rejection predicate failures retain `TransportError::JavaScript`,
including their original name/message and failed or effective request. They do
not consume the retry stamp or schedule another attempt. The actual-source
TypeError witness and native rejection/resend/read/write regressions live in
`tests/signature_error_kind.rs`; `scripts/signature-error-kind-control.py`
restores the original message-only wrapper to prove the failures, then restores
exact source bytes and requires every witness to pass.

The signature/remote-operation accepted batch observes the four retry helpers and
the RemoteSignerRequest discriminator plus its originating response decoder.
Native fields, stamp/callback order, failed-config replay and timeout effects
remain distinct from wire encoding. See ACCEPTED_API_REVIEWS.md; run the nine
source-disconnect controls serially and require named native failures with exact
restoration. The source oracle executes the maintained helpers and actual Axios.

Default HTTP query rendering uses `query::axios_default_query` before physical
I/O. It preserves native NaN/infinities until pair conversion, uses bracket paths
for nested records/arrays, and applies Axios buildURL encoding. Metadata `{}`
values retain their JSON.stringify phase. Custom serializers and HOW frame params
keep their existing ownership. See `QUERY_PARITY_EVIDENCE.md` for executed source,
HTTP witnesses, and the remaining reqwest apostrophe normalization.

`rate_limit::RateLimiter::acquire` is the cancellation-safe FIFO slot shared with `RateLimitedAdapter`. Native clients acquire it before signing each attempt; the adapter retains the same window, failed-request accounting and effective receipt. Dropping a pending acquire removes its ticket without consuming capacity. `tests/p2c.rs::signing_rate_slots_release_cancelled_waiters_without_spending_capacity` exercises the direct gate.

Native HTTP failures inspect connector causes before assigning a network code.
DNS lookup refusal and expired certificates remain non-retryable under the
maintained network-read predicate; temporary DNS and genuine connection refusal
retain their retry eligibility. `native::network_tests` exercises the physical
ReqwestAdapter and read retry policy with an injected resolver and a real local
TLS handshake. Its ephemeral certificate setup needs Node and openssl; it commits
no certificates or private keys. The installed Axios and maintained predicate
witness is `scripts/native-network-source.cjs` with SAFE_CEX_TYPESCRIPT_PATH set.
The connector's erased DNS context is pinned by that native regression, while
rustls certificate failures are identified through typed nested io causes.
Specific rustls expiry/validity/revocation/signature/purpose categories map to
Node-style codes. Hostname mismatch keeps ERR_TLS_CERT_ALTNAME_INVALID and its
installed retry eligibility, rather than inventing a blanket TLS retry refusal.
Other InvalidCertificate categories use CERT_REJECTED to retain
the certificate refusal category; rustls UnknownIssuer cannot identify every
OpenSSL self-signed/chain/leaf label, so exact label equivalence is unclaimed.

Bybit signature and network retries run through its constructed venue factory and
the shared signature_rejection module. The obsolete retry.rs compatibility
engine and the tests that only exercised it have been removed.

HOWWebSocket, RelayWebSocket, TransportRegistry and TealstreetTransport have
reviewed accepted contracts for all fourteen compiler-discovered APIs. Ten raw
entrypoints execute twenty-seven independently authored native observations:
stored endpoints, actual auth/frame bytes, latency fields, relay close events,
instance-provider precedence and exact upgrade headers. Typed routing/host
capabilities remain classified separately. The fixture only records host I/O;
it does not replace protocol or provider bodies. Run
`node --test sdk-rs/safe-cex-parity/scripts/accepted-transport-socket-native.test.mjs`
from the repository root. Its four production disconnect controls compile,
require named observation failures and restore exact bytes; run it serially
with every other source-sensitive probe or Cargo command in this worktree.
