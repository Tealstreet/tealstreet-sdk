# Paradex public refusals and Lighter public catalog

Owner codex-zs5e83, plan-first from actual `ba3d7da1d3a` on
`feat/rust-public-read-tail`. Overseer explicitly groups these last four workable
sites in one small draft. Lighter/GRVT local history drafts remain separate.
Read RUST_REQUEST_CONTEXT.md and both native venue RequestContext plans.

## Ownership and inventory

Fresh open-PR file lists show no claim on Paradex public.rs/depth_query.rs or
Lighter lib.rs and the dedicated new test paths. #1967 holds Lighter sign/trading/
Cargo and its recipe test; #1988 holds history only. All remain untouched. Sam's
Paradex B/C decision holds private signing/account/history; this slice never
restructures signing or private state. No shared source/checker changes.

| Path | Before F/O | After F/O | Move |
| --- | --- | --- | --- |
| Paradex public.rs | 7/7 | 5/5 | Two synchronous candle refusals |
| Paradex depth_query.rs | 1/1 | 0/0 | Initial managed book command admission |
| Lighter lib.rs | 1/1 | 0/0 | Public startup catalog request |

Paradex family44F/51O becomes41F/48O. Lighter actual39F/41O becomes38F/40O
independently of #1988's seven-site prune. Foreign maps remain exact incoming.
Remove only these singleton fingerprints in their original paths:
- Paradex public: `03068ad7442b394da421e7d1e0e04829b66c8bf11ddefd0dfdf69862ab1fa97f`
- Paradex public: `da04770a10b1e89bc8090a23a76b5cb0ac39bd576ec7a35134c4bb8873d55eaf`
- Paradex depth: `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa`
- Lighter lib: `1edfac341aacb74280c04d466785c3f6d76a4871da14449c5712b67a351a608e`

## Paradex move

Only FetchCandles unsupported-interval and unknown-market branches admit genuine
initial Read/None/20000ms/no-pacing/Never at their existing local branch. Preserve
interval-before-market decision, native exact messages and no remote-fallthrough
Context. Shared Rejected emits one typed failure; Revoked returns without taking
another owner. Successful admission fails its exact original root. Do not change
remote klines/TradingView fallback, polling or callbacks.

BookQuery replaces http_context(Scope::Command(id), policy) and numeric issuance
fallback with admit_http_command(policy), preserving its existing20s Read policy,
Pending root, managed GET bytes, response validator, codec and typed errors.
Any admission error returns without numeric fallback. Successful remote book
behavior is unchanged; admission capacity/clock now has the shared error without
a financial outcome instead of native local NotSent.

## Lighter catalog move

The one native markets(fallback) request becomes an unsigned managed Background
Read, credential None,20000ms,no pacing,Never. Each physical startup/fallback
request has its own Background root, matching the existing physical20s HTTP
window. There is no command/caller-wait/joined authority or multi-page Plan.
Keep exact GET hosts across Lighter/Robinhood main/testnet, no headers/body,
orderBookDetails then orderBooks, and ArrayAt::Envelope order/details and books.

Pending::Markets gains that exact Context. request_managed plus stream_response
carry the unchanged native payload. Admission refusal uses existing shared
startup backoff for the catalog key; preparation/transport/native-response
failure finishes only that attempt and preserves native primary->fallback then
existing startup.failed(true) behavior. No venue timer/queue/retry copy is added.

Before decoding any Markets item, before response-envelope/fallback side effects,
and before stream-error handling, validate_background_read_http_delivery on that
same root. This fences exact current managed callback and deadline. Preserve all
native row checks, spot filtering, staging, metadata, success/startup/account/
connect/poll sequence. Publish native catalog rows/metadata while validated and
retire the Background context after publication, before subsequent startup I/O.
Failure cleanup consumes the same pending root; engine typed failure forwarding
already delegates to the existing request-failure hook. No unrelated Pending,
financial reducer, websocket or signed path changes.

Existing fixtures that expect startup raw Action::Http must drain through the
shared prepared_actions at their exact native time. Only those test transport
drains change; every existing assertion, timer expectation and wire body remains.
No copied release helper in production and no fixture bypass of real claim.

## Original REDs and working controls

Commit two new dedicated test targets before production, then execute assertion
REDs on unchanged source. Paradex controls: genuine256 retained peers, each
candle local refusal and book entry capacity/clock typing, retire/reuse and live
duplicate owner, unsupported interval before unknown market, finite local native
messages, exact existing managed book GET/schema, and saturated known-market
remote candle legacy request bytes/no unused root. Preserve old tests.

Lighter controls use actual Engine start and Prepare/Bind/Claim: zero HTTP before
claim, exact unsigned public intent/header/body/host bytes, native nonempty
catalog codec/precision/spot filter, orderBooks fallback on native failure, no
publication from malformed/failed prefixes, prep refusal and released timeout,
late/duplicate response inert, repeated attempt retirement, and native backoff.
Keep identifiable native controls GREEN even while managed assertions RED; do not
count compile failures as RED evidence. Cover all four deployments. Deeper suffix
assertions behind a first RED are disclosed until implementation executes them.

## Reviews and gates

Independent source/adversarial, caller and full-map plan/test/implementation
reviews. Architecture gate: No; consumes shared command entry, Background delivery
validation, managed transport and existing startup backoff. No local cross-cutting
policy or engine seam. Scope is the three source files, dedicated new targets,
necessary existing fixture drains, exact own manifest keys and append-only docs.
Run scoped targets, native Lighter/Paradex/Binance, strict touched-crate Clippy,
formatting, bans and both request-context checks. CI owns WASM/replay/size/timing.
Open one draft, one>=120s watcher; green CI/reviews and explicit train window plus
a fresh separate own-path/own-key guard precede head-matched merge.
