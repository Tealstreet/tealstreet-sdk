# KuCoin futures and Spot RequestContext inventory and migration plan

Owner: codex-9czdjh; overseer: claude-ddi2il.

Snapshot inventory: **84 occurrences / 74 path-hash rows / 9 files**.

## Shared authority contract and review gates

This is a source inventory and migration plan, not a claim of completed adoption.
The snapshot is `fa5d5fb90c46f1c9d2984e1ed9c451b5f8370785`. Paths below are relative
to `sdk-rs/`; line numbers locate that snapshot, and full scanner hashes identify
ratchet rows. A repeated hash has an explicit occurrence count and all source lines.
Conservative macro/field matches are identified separately from dispatch/completion
calls; preserve their assertions and semantics rather than removing them to quiet
the checker. No inventory exemption, numeric completion helper, fabricated Context,
credential, signer, HTTP operation, or replacement venue pager is permitted.

Use [RequestContext](RUST_REQUEST_CONTEXT.md), the existing Binance managed lifecycle,
engine row admission, and the shared history algorithms. A command enters through
the genuine engine command-entry authority before native I/O. The reviewed API must
provide denial before Context issuance; a refusal already completed by the engine
returns immediately. Do not add another legacy `fail` fallback. `Error::venue_error`
from reviewed #1808 converts Context errors; it grants no command-entry authority.
Validate the original completion and exact delivered leg before decoding effects,
Store/event publication, history admission, incompleteness marking, or terminal
completion. Duplicate, superseded, and reused-ID deliveries grant no authority.
An expired success grants no Store/send rights; engine-admitted timeout/retirement
may complete the current original root through generation-only completion authority.
Use generation-bound `done_context`/`fail_context`, never the numeric command ID.

Single-response reads need no speculative Plan. Multi-request work requires the
shared Plan admission/journal owner, including children, retries, original deadline,
partial IDs, and cancellation. Background work needs a retained Scope and engine
session/operation authority, with cleanup before callbacks and late deliveries.
History uses the shared Context-bearing Completion/HistoryRows/CursorHistory bridge;
keep `finish_with`, reject, accepted-only recovery, and the original root. Native
codecs supply native rows/codes only. Move existing error mappings into the shared
classification table; do not add status ladders, outcome aggregation, local retry
policy, effective/unknown demotion, or completeness logic. Signers export reviewed
SignStep recipes with opaque identity; preparation preserves native bytes, static
headers, broker fields, credential role, route, and signing-at-dispatch behavior.

Each implementation area gets a separate PR: failing regression tests in one
commit, source and exact venue-only ratchet shrink in the next. Append landed caller
behavior to RUST_CUTOVER_CALLERS.md, audit/quality evidence to their appendices, and
update this plan/index as appropriate. Recheck every open PR file list immediately
before source/test edits. Do not edit a source/test file claimed by another PR.
Shared docs and the ratchet are shared by design. At integration keep incoming docs
first and this appendix once verbatim; keep incoming inventory entries and reapply
only this venue's actual prune, then regenerate/verify the checker.

Independent source/adversarial, caller, and full-map reviews must establish exact
inventory coverage and answer: “Is this a per-venue fix of a cross-cutting concern?”
The intended answer is no: missing authority goes to the shared owner. Locally use
`CARGO_BUILD_JOBS=2`, focused touched-crate plus Binance tests, strict scoped clippy,
fmt and both request-context checks. CI owns WASM/replay/performance gates. No merge
without overseer window and a clean revert guard. Docs-only inventory commits do
not claim runtime tests or changed caller behavior.

## Scope, ownership, and phases

Futures/common contributes 52 occurrences and Spot 32. The open-PR file audit found
no KuCoin source/test collision. History owner codex-cxxyqr explicitly disclaimed
KuCoin source/capability/history edits; its saved capability proof changes only host
harness tests. Preserve Spot `account_margin_type=true` and the successful
SetMarginType no-op. Do not change capabilities during lifecycle adoption.

1. Single requests first: futures candles, Spot candles/catalog/tickers, identity
   `/api/v1/user-info`, Spot account `/api/v2/user-info`, and supported local terminal
   results, only as their shared prerequisites land. `public()` is conditionally
   primary-signed when primary credentials exist; it is not uniformly unsigned.
   Its keyless headers are ordered X-CHANNEL-API-CODE=3zexp then Content-Type JSON.
   Spot streamed public reads use the same static headers. These need the shared
   unsigned-header seam (codex-5c57jl), entry admission (codex-uufub4), and for keyed
   requests the reviewed HMAC SignStep family. Do not drop headers to unblock.
2. Single-page Spot fills use shared ReadHistory/Completion and the signer, without
   inventing a Plan. Futures public trades have one successful response but delayed
   retries; they currently use signed_streamed and secondary GET selection when keyed,
   unlike public(). Preserve that distinction and original retry/deadline ownership.
3. Account refresh, active-order census/pages, symbol/metadata then leverage chains,
   fills/orders/ledger windows, funding windows, and trading dependencies wait shared
   Plan/row/history/signer admission. Public/private bullet POST is Session authority,
   coordinated with the session/signer owners, never inferred from HTTP method.
4. Batch writes, mode preflight, cancel fallback and replacement verification use
   CommandWrites journal facts and shared plans. No concrete multi-owner coalescing
   was proved in this source; do not impose an unnecessary group blocker.

HMAC owner codex-5c57jl reserves sign.rs and narrow signed issuance/credential seams
in lib.rs and spot/mod.rs; coordinate exact regions before changing shared helpers.
Error-classification owner claude-dazzy3 is paired for wire.rs write_failure: the
shared KUCOIN CodeTable must prove decoded 400002 on HTTP401 overrides status,
HTTP418 write is rateLimited, and failed multi-order PlaceOrders retains accepted
IDs through the journal. This is a later managed-write proof, not a local read shim.

## Caller behavior and regression matrix

Futures candles preserve native response order, seconds, native volume, minute
query granularity and millisecond bounds; do not add an unknown-market guard.
Spot candles preserve ascending sort, its distinct second bounds, limits and
normalization. Reject/empty precedence remains exact: futures unsupported client-ID
lookup before credentials, while Spot queries check credentials before their
specific branches. Preserve Spot fixed leverage `1`, settings no-ops, empty cancel,
unsupported public trades, and empty history/income results.

Spot client lookup pages active orders; a transport failure deliberately emits its
diagnostic then returns the cache-backed found result. Preserve both behavior and
cache fast path. Spot fills is one physical page, not a pager. Futures funding uses
60-day windows, inclusive timestamps, same-timestamp coverage refusal, bounded
retries and 350ms scheduling. Retain large numeric offsets as exact integers,
accepted rows on later failures, and owned incompleteness before terminal completion.
Account snapshots need revision/census validation and poll cleanup before publish.

Tests must pin keyed/keyless/secondary identities and byte-exact headers/routes;
preflight precedence and local no-ops; expiry, stale response, duplicate/reused ID,
preparation rejection, transport failure and malformed envelopes; candle shapes;
paged history coverage, cursors, empty/zero-limit and accepted-only failure; and
write partial IDs with engine aggregation. Existing scanner-matched assertions
remain proof, not migration targets to delete.

## Exact scanner inventory and engine-path map

| Source at snapshot | SHA-256 scanner key | Count | Classification and destination |
| --- | --- | ---: | --- |
| `venues/kucoin/src/account.rs:794` | `0763c380b5595f2dd5a739dc559d90a93fad2f0d4ea0c72dd26854c9a87c14e7` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/kucoin/src/account.rs:792` | `0cb27533973d0ad65fbf7717bc8f06498a1449efee8ca32173d60c0db9ac46b1` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/kucoin/src/account.rs:804` | `c22f9a403657e792ba5fc1ff078bc6061bfbb345e9724d6ebc11213628568639` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/kucoin/src/funding.rs:143` | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 1 | call: Funding preflight/empty terminal -> original Read Context; windows -> shared ReadCursorHistory + Plan + signer. |
| `venues/kucoin/src/funding.rs:128` | `4403f13e4ebee41090fae68af15a7e2b9dc365acd15180931a19617f04e019ff` | 1 | call: Funding preflight/empty terminal -> original Read Context; windows -> shared ReadCursorHistory + Plan + signer. |
| `venues/kucoin/src/funding.rs:120` | `7492685f19344bbb5f71aaa9ce5ddd5a6c8334ec01afea4272d430e24a8c8ff7` | 1 | call: Funding preflight/empty terminal -> original Read Context; windows -> shared ReadCursorHistory + Plan + signer. |
| `venues/kucoin/src/funding.rs:197` | `8c5c5f6e886f77e292bbb10a99aaf8c16333fdd60c7af0fb4880afcc0f040560` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/kucoin/src/funding.rs:134,138` | `9a60e91368234580b182f6a992676102fae962a3e073ccdc5c5af671782ab1ef` | 2 | call: Funding preflight/empty terminal -> original Read Context; windows -> shared ReadCursorHistory + Plan + signer. |
| `venues/kucoin/src/funding.rs:112,116` | `b794452f2a30bb8d7962980aa180699a68c7d0b92f6cba189f507f886f6dfca9` | 2 | call: Funding preflight/empty terminal -> original Read Context; windows -> shared ReadCursorHistory + Plan + signer. |
| `venues/kucoin/src/funding.rs:124` | `e0e5973853e07ba0ab695de8b9f063ba7b1e66042c92d7e34545cabefa62b30b` | 1 | call: Funding preflight/empty terminal -> original Read Context; windows -> shared ReadCursorHistory + Plan + signer. |
| `venues/kucoin/src/history.rs:300` | `2014ae71f12da2dbe77247605b6ed232a347804065ec697883c6453be4d684b2` | 1 | call: History preflight/empty terminal -> Read Context; native history -> shared Completion/CursorHistory and exact-leg admission. |
| `venues/kucoin/src/history.rs:370` | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 1 | call: History preflight/empty terminal -> Read Context; native history -> shared Completion/CursorHistory and exact-leg admission. |
| `venues/kucoin/src/history.rs:256` | `557a3c547f8fbe25d2435df46a5dcdfedc8cbc48a7bd84f93300dd188bfb60bd` | 1 | call: History preflight/empty terminal -> Read Context; native history -> shared Completion/CursorHistory and exact-leg admission. |
| `venues/kucoin/src/history.rs:996` | `6dc8ff5c9a3f54afd832946af7fc2e777f459ef0ebb8da410821660b9044f481` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/kucoin/src/history.rs:294` | `7492685f19344bbb5f71aaa9ce5ddd5a6c8334ec01afea4272d430e24a8c8ff7` | 1 | call: History preflight/empty terminal -> Read Context; native history -> shared Completion/CursorHistory and exact-leg admission. |
| `venues/kucoin/src/history.rs:290` | `760402c5deb4b7e740454abfc537c556868ba8de0d9474196df4bc18882ccdc3` | 1 | call: History preflight/empty terminal -> Read Context; native history -> shared Completion/CursorHistory and exact-leg admission. |
| `venues/kucoin/src/history.rs:1260` | `76cccead438e68521694435ebb3adb21716e5c650bdd63ae55312345486336e3` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/kucoin/src/history.rs:1141` | `770f2dcac9b528f2bacfc9329e543f5215a004fb671054775207c827e04764a8` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/kucoin/src/history.rs:957` | `782dad616e9bbc6860101de1c4c5458533faa67d9b51a888ebd1dbfd205aa1ec` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/kucoin/src/history.rs:1210` | `ae699603109cbd1c71356f8a3a96c279698017af0d318744b47d63a2bf08aef4` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/kucoin/src/history.rs:306,310,332` | `fe59f0c530fb432f64301445ac4166bffdf59e67617abe41b8cd709527d3f06d` | 3 | call: History preflight/empty terminal -> Read Context; native history -> shared Completion/CursorHistory and exact-leg admission. |
| `venues/kucoin/src/identity.rs:78` | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | call: Single identity GET -> signed managed Call/Lifecycle + Context terminal; preserve native account payload. |
| `venues/kucoin/src/identity.rs:81` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | call: Single identity GET -> signed managed Call/Lifecycle + Context terminal; preserve native account payload. |
| `venues/kucoin/src/lib.rs:1336` | `0451c29993291234cad6e42a586a776c0df200276aac899eaf08c1753d2e83fb` | 1 | call: Candle/unsupported preflight or terminal -> Read Context + managed lifecycle; preserve range/interval and native rows. |
| `venues/kucoin/src/lib.rs:1330` | `20065b0c046ae217abcd13cedaf9c40d0792cb3aa9492fad885b5014635ccb0a` | 1 | call: Candle/unsupported preflight or terminal -> Read Context + managed lifecycle; preserve range/interval and native rows. |
| `venues/kucoin/src/lib.rs:1229` | `af50b0be7d7a0e63d0686b381091e4fd428fb7b4980c453864ee995b501543b0` | 1 | call: Candle/unsupported preflight or terminal -> Read Context + managed lifecycle; preserve range/interval and native rows. |
| `venues/kucoin/src/lib.rs:286,324` | `b7236676fb5159713524fe49af418c62b87162e905dab02bef27a1b2badde5a3` | 2 | call: Signed dispatcher -> managed Call/Lifecycle + reviewed family SignStep; child Plan where multi-request. |
| `venues/kucoin/src/lib.rs:334` | `bac8c36ebafa2042f49567b5bfb145dc6285c54d802542bcef1aa1d54d94955a` | 1 | call: Conditionally primary-signed public GET -> managed lifecycle + signer or static-header seam; preserve keyless headers. |
| `venues/kucoin/src/lib.rs:1127` | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | call: Candle/unsupported preflight or terminal -> Read Context + managed lifecycle; preserve range/interval and native rows. |
| `venues/kucoin/src/lib.rs:1128,1160` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 2 | call: Candle/unsupported preflight or terminal -> Read Context + managed lifecycle; preserve range/interval and native rows. |
| `venues/kucoin/src/lib.rs:363` | `f6fd9085f9b1b305170e32707e6244a6773605cd81588f17ab52b6a17cb6e08a` | 1 | call: Bullet public POST -> shared Session authority/lifecycle and conditional signer; not inferred Read. |
| `venues/kucoin/src/secondary_tests.rs:140,187` | `0597ddd241d06b439fbe4e473127392d5b647a9d4437fc1a4b903b974f21bf94` | 2 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/kucoin/src/secondary_tests.rs:173` | `159f5704bcac34411f65ad8cc4e2e106a4d8575fedd68580800396d53c3c9ce7` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/kucoin/src/secondary_tests.rs:87` | `65ff396da1ed90ecf05d1827e7770baf9314abb1ab900b929a600b9142d9b401` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/kucoin/src/secondary_tests.rs:174` | `7af1094316bcf84e9fbbbf0a9424df23be5feac988016338a63a93369f08ae3b` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/kucoin/src/secondary_tests.rs:179` | `856927040f21fefa7ac348eca86cf760ea2b34f71b2a42785d71a7eaf490b319` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/kucoin/src/secondary_tests.rs:230` | `8eef6157c4aa384dd6cf16bd4fb4d8f28963578b97454ea94d8d63d988497812` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/kucoin/src/secondary_tests.rs:77` | `a65fdc4b75f9441e9693fd7c31ac9019dbf1d2ca3215e6e557105b2e9223db3f` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/kucoin/src/secondary_tests.rs:229` | `a7295c3cde6850a230bb760a9465683ccd718556f87cab40a9283bb16899df53` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/kucoin/src/secondary_tests.rs:131` | `e57766872e3db7f45d338b12e52551e4cafa21312459083c934b95009cae10b9` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/kucoin/src/spot/mod.rs:1649` | `06eab8c4791cec49c676233d2e64048f1eaefe972dddfc52dfdffa06b4240730` | 1 | call: Lookup/cache terminal -> original Read Context; preserve diagnostic/cache-found fallback and paged census Plan. |
| `venues/kucoin/src/spot/mod.rs:1543` | `08746e27d5c5b4fa82d1534a206c0f5932b85da2bc45429c8e9bd888544ebff4` | 1 | call: Spot read/preflight terminal -> original Read Context; preserve candle/account/empty/unsupported branch semantics. |
| `venues/kucoin/src/spot/mod.rs:426` | `2485f9b0677fc07b9dc4f3202a0dbfc7dd3b13e82476cd509be3282e8ce4d863` | 1 | call: Active-symbol/order census -> shared Plan + row/revision admission + signed managed lifecycle. |
| `venues/kucoin/src/spot/mod.rs:1605,1676` | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 2 | call: Spot read/preflight terminal -> original Read Context; preserve candle/account/empty/unsupported branch semantics. |
| `venues/kucoin/src/spot/mod.rs:269` | `417cd86311e6261c7b1911896dc467829c4cfafd1fc48956cfbce153fbc17582` | 1 | call: Bullet public/private POST -> shared Session lifecycle with native signing distinction. |
| `venues/kucoin/src/spot/mod.rs:414` | `44284ddcfc94ad5da1cd7f5576dc01737ad7eeb0f75aa49c95bf42849d08790a` | 1 | call: Active-symbol/order census -> shared Plan + row/revision admission + signed managed lifecycle. |
| `venues/kucoin/src/spot/mod.rs:247` | `4d14eae20fff4dab766584ec10ba890cc51f49fbf86cf1e5d95860110212dbf1` | 1 | call: Unsigned streamed public GET -> managed lifecycle + static-header seam; row/envelope authority before publish. |
| `venues/kucoin/src/spot/mod.rs:1408` | `520659ca128f10f1f3f1a724ed704b116b8f56da364dcd28fd6aaabfc25c5872` | 1 | call: Lookup/cache terminal -> original Read Context; preserve diagnostic/cache-found fallback and paged census Plan. |
| `venues/kucoin/src/spot/mod.rs:1672` | `96134b6210eed5e21fe96c0412c596ffa47622ce39c4f6247960efe6dec683a2` | 1 | call: Spot read/preflight terminal -> original Read Context; preserve candle/account/empty/unsupported branch semantics. |
| `venues/kucoin/src/spot/mod.rs:1593` | `97e495223a50ee0b0214ca3f811598e16b758b9bb3eb780db321e78fc0c17442` | 1 | call: Spot read/preflight terminal -> original Read Context; preserve candle/account/empty/unsupported branch semantics. |
| `venues/kucoin/src/spot/mod.rs:1304` | `9eb2532ffa305903307527ed26561752ae2716a465c7347f6bb03d38987efb3d` | 1 | call: Spot read/preflight terminal -> original Read Context; preserve candle/account/empty/unsupported branch semantics. |
| `venues/kucoin/src/spot/mod.rs:1301` | `ae51780dba21824a7515bfffef90bd4f78599ea6af3cc40ef36db98ec6dc93a4` | 1 | call: Spot read/preflight terminal -> original Read Context; preserve candle/account/empty/unsupported branch semantics. |
| `venues/kucoin/src/spot/mod.rs:238` | `b7236676fb5159713524fe49af418c62b87162e905dab02bef27a1b2badde5a3` | 1 | call: Shared signed/unsigned dispatcher -> managed lifecycle, reviewed SignStep/static headers; preserve credential selection. |
| `venues/kucoin/src/spot/mod.rs:400` | `bf606094d92ae7fcf6e97dd3f9f690ff00d2954558e1c92671c1c455c0535065` | 1 | call: Balance read -> managed signed lifecycle + row/revision admission under account Scope. |
| `venues/kucoin/src/spot/mod.rs:1337` | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | call: Spot read/preflight terminal -> original Read Context; preserve candle/account/empty/unsupported branch semantics. |
| `venues/kucoin/src/spot/mod.rs:1338,1426` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 2 | call: Spot read/preflight terminal -> original Read Context; preserve candle/account/empty/unsupported branch semantics. |
| `venues/kucoin/src/spot/mod.rs:1622` | `de4e8a9f01fa2526af01b897df7e71eb6422d5bda2faabb22527e73503e6a5a6` | 1 | call: Single-page fills -> signed managed lifecycle + Context ReadHistory/Completion; no speculative pager. |
| `venues/kucoin/src/spot/mod.rs:1277` | `f9cbb2adfc98c4ca1e0d379b2b1c90b52a3e91fd61f8beaebb998423812ddf8d` | 1 | call: Lookup/cache terminal -> original Read Context; preserve diagnostic/cache-found fallback and paged census Plan. |
| `venues/kucoin/src/spot/trading.rs:228` | `06eea309fa05c237f809ccb4c45df7a36798cfb0bcb6ace2f22b4b3d7d6f0ef3` | 1 | call: Spot trading/local setting terminal or dispatch -> command entry; single no-op Context, writes use shared Plan/journal + signer. |
| `venues/kucoin/src/spot/trading.rs:411` | `110dc698dbd0bec05d051c891a2958a79e8a915b14a70df63054cc306d64a8f0` | 1 | call: Spot trading/local setting terminal or dispatch -> command entry; single no-op Context, writes use shared Plan/journal + signer. |
| `venues/kucoin/src/spot/trading.rs:377,390` | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 2 | call: Spot trading/local setting terminal or dispatch -> command entry; single no-op Context, writes use shared Plan/journal + signer. |
| `venues/kucoin/src/spot/trading.rs:397` | `33e21f13f871bdbb7268e01da0006a69a903f16f2d8daa34972b49211d81ee6f` | 1 | call: Spot trading/local setting terminal or dispatch -> command entry; single no-op Context, writes use shared Plan/journal + signer. |
| `venues/kucoin/src/spot/trading.rs:438` | `3c98818bcbb2b5da43180ebf643ff2a24bef6f432d6af53d5f601eef34c34547` | 1 | call: Spot trading/local setting terminal or dispatch -> command entry; single no-op Context, writes use shared Plan/journal + signer. |
| `venues/kucoin/src/spot/trading.rs:395` | `97e495223a50ee0b0214ca3f811598e16b758b9bb3eb780db321e78fc0c17442` | 1 | call: Spot trading/local setting terminal or dispatch -> command entry; single no-op Context, writes use shared Plan/journal + signer. |
| `venues/kucoin/src/spot/trading.rs:420` | `c9f18250fdcb22a64156d93e3709f89aa88fb3912e9f4cb92cb28cddca2173bf` | 1 | call: Spot trading/local setting terminal or dispatch -> command entry; single no-op Context, writes use shared Plan/journal + signer. |
| `venues/kucoin/src/spot/trading.rs:157` | `d0677fb68bd1966d34f4d686f19ab321881f696d903f33b76f0ad52aac4b3902` | 1 | call: Spot trading/local setting terminal or dispatch -> command entry; single no-op Context, writes use shared Plan/journal + signer. |
| `venues/kucoin/src/spot/trading.rs:386` | `d8877910dd4935bc942e96557431f8cb07f3f22f072bc567edd25ba564f4dcf9` | 1 | call: Spot trading/local setting terminal or dispatch -> command entry; single no-op Context, writes use shared Plan/journal + signer. |
| `venues/kucoin/src/spot/trading.rs:239` | `f1f763932a88f842e8e92da7aea07af8d01b3644aa720ef22e94000247d858b6` | 1 | call: Spot trading/local setting terminal or dispatch -> command entry; single no-op Context, writes use shared Plan/journal + signer. |
| `venues/kucoin/src/spot/trading.rs:159` | `fc46e4dc9fea9911455ee3e3f7822ea0a3c903d8314f5fa82214941c0a64d9f0` | 1 | call: Spot trading/local setting terminal or dispatch -> command entry; single no-op Context, writes use shared Plan/journal + signer. |
| `venues/kucoin/src/trading.rs:670` | `1beb1b17521869e43234e5466812f0244a3d2d12ef32593b95c177b1ef8f29db` | 1 | call: Write preflight/aggregate terminal -> command entry + shared Plan/CommandWrites journal; retain partial IDs. |
| `venues/kucoin/src/trading.rs:666` | `2f0770d2ea5f5246cd2ddcd35b614bce738e564c2d95193ff5a84eb8479811e5` | 1 | call: Write preflight/aggregate terminal -> command entry + shared Plan/CommandWrites journal; retain partial IDs. |
| `venues/kucoin/src/trading.rs:774` | `a3929c62720c7893d6477453bb8092ee1dc649a8b5024ffd9867b8715c3afc2f` | 1 | call: Write preflight/aggregate terminal -> command entry + shared Plan/CommandWrites journal; retain partial IDs. |
| `venues/kucoin/src/trading.rs:781` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | call: Write preflight/aggregate terminal -> command entry + shared Plan/CommandWrites journal; retain partial IDs. |
| `venues/kucoin/src/trading.rs:815` | `e2059285572e975717f09275846f33678d7bf1e78ddd4d40100025ab825c118f` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |

This table exhausts this venue’s current ratchet rows. Planning changes no source,
tests, capability flags, caller behavior, or inventory entries.
