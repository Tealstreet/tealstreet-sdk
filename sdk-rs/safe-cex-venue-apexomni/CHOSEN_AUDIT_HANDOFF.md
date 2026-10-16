# ApexOmni CHOSEN audit: hard-stop handoff

Accepted original FINDING cohort: **10 behaviour /57 rationale over67 rows**.
This continuation has **no newly runtime-validated behaviour/rationale split**.
The original73 CHOSEN-text rows were reviewed statically:63 bounded choice
rationales are supportable and10 rows need behaviour probes. Neither these
static dispositions nor the old CHOSEN text close the venue. The two retained
ignored-extra source scope survivors are still open. No production repair was
started before the hard stop.

## Scope and measured census

At reviewed payload76e69ebbe619af29b170981fe744a476a05a6ae1, recursive tomllib
parsing measured28 ported /140 divergent /0 pending,168 rows over16 manifests,
stopped file index15. Baseline d8772a9e8b42c71faf7f56f192642aa70bf90108 independently
identifies exactly67 FINDING-prefix and73 CHOSEN-prefix divergent rows. Match
manifest path, TypeScript symbol **and kind**: Market CONST is a target; its
separate ported TYPE row is not. The audit inventory has73 unique keys, stopped
index72, in audit/chosen-cohort.json. It copies no row declarations or notes.

Read-only sub-agent partitions covered28 exchange/utils rows through index27,
25 API/Market/signer rows through index24, and20 socket rows through index19.
Their static dispositions are26/2,24/1 and13/7 rationale/candidate respectively.
All three ran no Cargo, Node, source probes, mutations or writes. The earlier
constructor whitespace alarm was retracted after reading the shared helper:
js_split_whitespace preserves boundary empty segments and rejects those inputs.
Do not list it as a defect.

## Ten original CHOSEN rows needing probes

These are static source/native discrepancies, not measured runtime failures.
Existing chosen ownership, scheduling and typed-carrier costs do not establish
that these discrepancies are intentional. Classification must follow
[sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy).
Retain the independent costs under
[#owned-snapshot-vs-live-alias](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[#closed-typed-output-boundary](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[#shared-utf8-string-boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), and
[#forced-surrogate-conflict](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).
No canonical declaration is repeated here.

| Row | Candidate and required probe |
| --- | --- |
| createAPI | api_runtime.rs request_handler stringifies legacy URL before substring includes, whereas TS invokes includes on the original value. Array URL ['/v3/order-more'] with legacy data'a=1' can change the signature. Array URL ['/v3/set-initial-margin-rate'] should select multipart. Numeric URL17 should throw TypeError at the source includes call, with legacy and primary paths failing at different stages. Existing12 request fixtures omit these axes. Assert native error identity, headers/signature and ordered host calls. |
| ApexOmniExchange.cancelOrdersHelper | exchange.rs writes cancellation timestamps using runtime.now_ms; source uses Date.now. Inject distinct Date/virtual values and assert the native blacklist timestamp before wire. Fixing only the writer is insufficient; see the reader below. |
| ApexOmniExchange.fetchAndSetPrivateSnapshotHelper | lifecycle.rs unconditionally samples one clock and reuses it for the recent-WS guard and merge. Source conditionally samples the guard then samples again in the actual merge helper. Advancing draws1000/2001 with cached lastUpdate1000 can change the selected position. Assert both call counts and native selected fields, including absent last-WS state. |
| ApexOmniPrivateWebsocket.throttleFetchOrders | ws_private.rs catches fetch errors but lets Store update failure escape the Ok arm; source try/catch encloses both fetch and Store update. Use a throwing/fallible subscriber after a successful fetch, assert warning/result and committed Store state. |
| ApexOmniPrivateWebsocket.throttleFetchPositions | Same catch-scope gap for positions. Preserve the separate source disposal behaviour; it is not the same as the orders guard. |
| ApexOmniPrivateWebsocket.handleFillTopic | A non-array collection reaches a generic native expected-array Error; source data.forEach produces TypeError. Probe representable malformed account-frame collections and exact error kind/message. |
| ApexOmniPrivateWebsocket.handlePositionTopic | Same generic-array error versus source data.map TypeError. Probe exact error kind/message before downstream effects. |
| ApexOmniPublicWebsocket.handleInstrumentInfoEvent | Generic native array Error versus source non-iterable TypeError for malformed tickers. Probe actual instrument frames and native error identity. |
| ApexOmniPublicWebsocket.listenOHLCV | Early native String-price refusal masks later source arithmetic failure: after an initial candle, trade p:'20.25', v:{toString:null} should reach the source coercion error. Rollover reset is committed before later source volume failure but native clones until success; probe a subsequent valid trade to expose retained state. Preserve the independently priced typed close/alias costs. |
| ApexOmniPublicWebsocket.listenOrderBook | Source clears/rebuilds captured snapshot state incrementally; native clones and commits only after complete success. A valid first ask followed by a throwing level then a valid empty delta exposes different retained books. Assert decoded levels, amounts and totals after that continuation. |

Additional site outside the73: the already counted original behaviour row
ApexOmniExchange.fetchOrders expires cancellation entries using virtual time,
where source reads Date.now. Its old fixed-clock fixtures mask the distinction.
Probe the reader and writer together. This is an additional defect candidate in
an already counted row, not an additional numerator in the accepted10-row count.

## Rationale and harness limits left unfinished

The63 statically supported rows still need an accurately bounded runtime-backed
closure. Their manifest notes have not been rewritten or promoted. In particular:

- The five transaction signing notes must restrict ordering claims to admitted
  native payloads and explicitly price eager whole-payload refusal. Source may
  pack an invalid numeric amount before validation where native decoding refuses
  before host effects. This is not an enum-boundary license.
- assert's note incorrectly calls the separate assertAddress wrapper unported;
  the wrapper and mapping exist. That stale statement remains to be corrected.
- Market CONST alias TypeId proof and its37-member roundtrip loop are separate
  from direct decoded enum proof. protocol.rs's native-variant macro interleaves
  native and wire assertions; collect native assertions before wire for new proof.
- mutate_websockets.py expects25 restored tests while the current module declares27;
  its old receipts use unkeyed paths. It was inspected, not run successfully here.
- Generic exchange notes need row-specific ownership costs. fetchMarkets's
  omitted null-helper fallback is bounded to the fixed typed helper profile;
  fetchBalance/fallback return snapshots rather than source aliases. Keep those
  limits explicit rather than copying a shared declaration.

The private/public onMessage ignored-extra D800-to-D801 substitutions still
survive maintained-handler output assertions. Their call sites are
ApexOmniPrivateWebsocket.onMessage/jsonParse(packet.ignored) and
ApexOmniPublicWebsocket.onMessage/jsonParse(packet.ignored). The units are ignored
by those handlers; these are recorded scope findings, not passing negative gates
and not required-string-carrier proofs. Existing notes price the separate native
whole-frame losses (missing ASCII pong/update), but that rationale does not close
the requested survivor investigation. No new source or native probe was run.

## Delivery limits

Status rows moved0. Manifest notes, Rust production, tests and controllers are
unchanged by this handoff. Only the current read-only audit is recorded. Original
source/raw collection counts and accepted prior proofs are not new gate claims.
The branch started at76e69ebbe6; origin/feat/safe-cex-rust was fetched and remained
0286f63437e4320e8c12886d5fef8fc6dfa2eac3, whose merge was already present. The
coordinator's accepted batch merge703bc37382 contains that prior payload; it was
not present in origin at the fetch. No unreviewed production changes are added.

At the exact final handoff SHA the lane runs only cargo fmt --check and cargo
build --workspace --locked --jobs2. Their measured exits live in the SHA-keyed
sdk-rs/target/apexomni-stop-gates receipt. NOT RUN in this continuation: scoped
runtime tests, all mutation/source controls, source probes, host clippy, wasm,
wiring, layers, routing, extractor writer/check, full suites, FFI jobs, live
network/signing or CI. The central gates remain owned by the coordinator.
No push, PR or self-merge.
