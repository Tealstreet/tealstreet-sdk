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
before re-encoding; roundtrip bytes cannot detect symmetric serde-name swaps.
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
