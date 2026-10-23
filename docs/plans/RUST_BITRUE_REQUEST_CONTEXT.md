# Bitrue RequestContext inventory and migration plan

Owner: codex-9czdjh; overseer: claude-ddi2il.

Snapshot inventory: **53 occurrences / 36 path-hash rows / 5 files**.

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

No Bitrue source/test collision was found in the open-PR file audit. First area:
public WebSocket candles/trades. They are per-channel FIFO: only the first owner is
sent; a reply removes/completes one and sends the next. They are NOT a coalesced
operation. Obtain an initial Read Context from reviewed command-entry admission
before WS/timer I/O, retain it in the queued request, and complete via Context.
This API queues no fake HTTP. The core owner confirmed the shape but has not yet
provided a reviewed source pin. Preserve native send/correlation/timer behavior.

Second area, after the signer/header/routing cores: single bracket/max-leverage GET,
client-ID lookup/cache fast path, one-page fills. Markets goes through the same
HTTP dispatcher but is conditionally primary-signed and explicitly Primary-routed
even keyless. It always sends ordered Content-Type JSON and X-Referer=Tealstreet.
Do not replace this with default Public routing or lose the static headers. The
credential owner claude-m26ps4 supplies PrimaryRead(Catalog) for keyed catalog reads;
keyless Primary egress requires explicit transport intent in the shared core.
Unsigned-header owner is codex-5c57jl, also HMAC owner; private requests wait its
reviewed Bitrue SignStep and credential construction, with no method inference.

Later account refresh/census and trading sequences require shared Plan/row/journal
admission. Listen-key POST and keepalive PUT are Session mutations, not Public/Read.
FetchLeverage really is coalesced: account.rs leverage_queries joins multiple command
owners onto one Positions/account response. Group owner claude-fe7lgm must supply
per-owner original completion/deadline authority, exact shared response authority,
and typed retirement so one expired owner cannot suppress siblings. No local group
implementation. Keep this site last.

## Caller behavior and regression matrix

Candles preflight order is interval, symbol, socket, range; trades check symbol then
socket. Preserve cap32, the 5000ms total deadline including queue wait, explicit
cb_id correlation and existing empty-cb_id compatibility. Timeout closes the public
socket and fails all pending owners because an uncorrelated reply cannot safely be
assigned afterward. Test two owners on one channel, wrong cb_id, duplicate explicit
reply, empty cb_id, old timer after ID reuse, disconnect/reconnect, capacity and
queue-wait expiry. Do not claim ambiguous empty-cb_id duplicates can be distinguished
without a protocol change. Preserve Singapore candle timestamp parsing, sorting,
deduplication, range/tail filters, volume multiplier and native public-trade units/IDs.

HTTP preserves native -1022 one-time re-signing and up to three eligible GET/listen
transport retries: do not blanket RetryNever or reset the root deadline. FetchMaxLeverage
preserves bracket cache/market-limit effects and malformed response retaining known
limit. Client lookup keeps found results as `{found:true, order}` with no `definitive`
field; misses are `{found:false, definitive:false}`, including keyless cache paths.
Maximum-leverage reads validate symbol before credentials; fills validate credentials,
then range, then symbol. Fills is a single recent physical page, with full-page filtered
coverage refusal, shared HistoryRows staging plus native trade-ID dedupe, units, seconds/milliseconds and staged
prefix rollback. Unsupported complete order/income history remains explicit.

Trading preserves unsupported margin/hedge-mode errors and successful one-way no-op,
initial/dequeued preflight, optimistic cleanup, independent continuation, accepted
IDs and definitive-refusal-only recovery. Replace effective/cancel_unknown decisions
with journal facts after the shared prerequisites, not another local ladder.
Conservative `pending.request` fields are not ten HTTP dispatchers; migrate them only
with the corresponding pending representation and preserve matched test assertions.

## Exact scanner inventory and engine-path map

| Source at snapshot | SHA-256 scanner key | Count | Classification and destination |
| --- | --- | ---: | --- |
| `venues/bitrue/src/account.rs:1454,1482` | `17eccb5b6b6d70935f1f5ba5db37d97212ca0bfad15e0c30b719b58a6bbdab89` | 2 | call: Leverage preflight/terminal -> original owner Context; FetchLeverage needs shared group/Positions authority, MaxLeverage stays single bracket GET. |
| `venues/bitrue/src/account.rs:1712` | `187ae7a49b233001f96c827642f4839ad9e3ea4fd81b9b62848d3900055afced` | 1 | call: Max-leverage single bracket read -> signed managed lifecycle + original Read Context; guard cache/limit publication. |
| `venues/bitrue/src/account.rs:2017` | `1b3029882eb1af7175e494797938adedfb4b3c6cbd38573c98c2e4940415d89b` | 1 | call: Max-leverage single bracket read -> signed managed lifecycle + original Read Context; guard cache/limit publication. |
| `venues/bitrue/src/account.rs:1643` | `4f19e9de3124938051c48304f8476b059f9928dd6f790df05a27c9f821512d93` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/bitrue/src/account.rs:1710` | `985d5251ce2c2f3e0d27d6c46def63a68db127135e27d9e2917a1923e354a48f` | 1 | call: Max-leverage single bracket read -> signed managed lifecycle + original Read Context; guard cache/limit publication. |
| `venues/bitrue/src/account.rs:1639` | `adebf191cda07505ad7d8529e3d1e50353f7a3251988cfcd01ba53d93563b444` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/bitrue/src/account.rs:1458,1486` | `b39921dbd50796a60a4e2f21febec858aef747570716300c27d2ec5c5512d0ad` | 2 | call: Leverage preflight/terminal -> original owner Context; FetchLeverage needs shared group/Positions authority, MaxLeverage stays single bracket GET. |
| `venues/bitrue/src/account.rs:1541` | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | call: Leverage preflight/terminal -> original owner Context; FetchLeverage needs shared group/Positions authority, MaxLeverage stays single bracket GET. |
| `venues/bitrue/src/account.rs:1542` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | call: Leverage preflight/terminal -> original owner Context; FetchLeverage needs shared group/Positions authority, MaxLeverage stays single bracket GET. |
| `venues/bitrue/src/account.rs:744` | `df3c1d81952c8a455f966081ae19fcca364d48093c5893e69a82108cac16906d` | 1 | call: Single lookup/cache terminal -> original Read Context; preserve found+order without definitive, misses with definitive:false. |
| `venues/bitrue/src/history.rs:162` | `1d929e91360e3f2b465b4f60d8024a7aa9f9167e8573e49457119fe4d06f634d` | 1 | call: History preflight -> Read Context; one-page fills -> signed managed lifecycle + shared ReadHistory/Completion. |
| `venues/bitrue/src/history.rs:146` | `3222c9dc7146bd368025519801a954c755cb187050b8dcca08eea2163dfb87f8` | 1 | call: History preflight -> Read Context; one-page fills -> signed managed lifecycle + shared ReadHistory/Completion. |
| `venues/bitrue/src/history.rs:140` | `3cdc4e2ec5596e8f05582d9ea14e49f5e810d97c769e5189330ab4292ee9b9b4` | 1 | call: History preflight -> Read Context; one-page fills -> signed managed lifecycle + shared ReadHistory/Completion. |
| `venues/bitrue/src/history.rs:158` | `b0723bf50b38aec58f93f9e0c3c86eb7adb6b09590d7255838d094a9bd119c37` | 1 | call: History preflight -> Read Context; one-page fills -> signed managed lifecycle + shared ReadHistory/Completion. |
| `venues/bitrue/src/history.rs:139` | `c26319aae17ffaa69a204fb71bea1ad4e83a1f01411ca808627909808f6be6ef` | 1 | call: History preflight -> Read Context; one-page fills -> signed managed lifecycle + shared ReadHistory/Completion. |
| `venues/bitrue/src/lib.rs:456` | `2c1945e8fe549e372b88fad9893fd4ccdb352a6f7ddbee111f657d0b47b94b3f` | 1 | call: FIFO WS read preflight/terminal -> initial Read Context, retained through queue/timer; no fake HTTP or group. |
| `venues/bitrue/src/lib.rs:333` | `34b6cbe60cecfea179e31290b2eb916f11a7555598d45b795b4633e6afc768fc` | 1 | call: Shared HTTP dispatcher -> managed lifecycle + signer/static headers/explicit route intent; preserve eligible retries. |
| `venues/bitrue/src/lib.rs:473,1101` | `4ae5b0ffbc25a89f4246071ca1693f16d4157262f64db6b70f13a7dd97fcca43` | 2 | call: FIFO WS read preflight/terminal -> initial Read Context, retained through queue/timer; no fake HTTP or group. |
| `venues/bitrue/src/lib.rs:551` | `5d6691e305370ef1b5e22426e494d28b28936de80d5d5af079e9deae52eb33f2` | 1 | call: FIFO WS read preflight/terminal -> initial Read Context, retained through queue/timer; no fake HTTP or group. |
| `venues/bitrue/src/lib.rs:1112` | `8290414cfeb1fa00b12f38b2747f35c6a79f2ee54904548bc762f71230edf7fa` | 1 | call: FIFO WS read preflight/terminal -> initial Read Context, retained through queue/timer; no fake HTTP or group. |
| `venues/bitrue/src/lib.rs:290,835,835,913,913,917,918,919,930,930` | `8d7195dd58da632e34e4dc75d236e49b6d1b89cc10c243ec7e670ec59b511d26` | 10 | field: Conservative field match; pending migration with shared HTTP lifecycle; preserve retry/dispatch semantics. |
| `venues/bitrue/src/lib.rs:736` | `8f75df3f520567e53f3f2e36f6503c9f12592536246710e2b9b9a22ee7e6c468` | 1 | call: FIFO WS read preflight/terminal -> initial Read Context, retained through queue/timer; no fake HTTP or group. |
| `venues/bitrue/src/lib.rs:550` | `aa71c1d92e34443bf9b99a67bc669e086e750105b16ed6b6d1835d2e7f2c2c0f` | 1 | call: FIFO WS read preflight/terminal -> initial Read Context, retained through queue/timer; no fake HTTP or group. |
| `venues/bitrue/src/lib.rs:477,1105` | `bca9da0516ee08e303dec8d8dde06029601ab163e9559171b2fc11062b7ab5b5` | 2 | call: FIFO WS read preflight/terminal -> initial Read Context, retained through queue/timer; no fake HTTP or group. |
| `venues/bitrue/src/lib.rs:437` | `eb08776bc0906e61326c6a1cd3cb4d53cafa9c7932e22316b59ea59737f9767b` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/bitrue/src/lib.rs:1097` | `f12f2537ab57e7f09386c1d94090d2cf74c04c947cdc0a54672430021b6ff935` | 1 | call: FIFO WS read preflight/terminal -> initial Read Context, retained through queue/timer; no fake HTTP or group. |
| `venues/bitrue/src/secondary_tests.rs:53` | `14ff7b932a6042d0adee80518f703a27a5e51c41add140215703e93820dc3475` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/bitrue/src/secondary_tests.rs:62` | `496bfb28799c729f0f335ccce50062d6a9ba06a581527e6a36a638ac44c84dc8` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/bitrue/src/trading.rs:854` | `09d284b47513158ba424efe6b85ef618c3f2649072f4211be5a3ec043ecc0e88` | 1 | call: Trading preflight/local no-op/terminal -> command entry; later Plan + CommandWrites journal for sequences/partial IDs. |
| `venues/bitrue/src/trading.rs:861,896` | `2f0770d2ea5f5246cd2ddcd35b614bce738e564c2d95193ff5a84eb8479811e5` | 2 | call: Trading preflight/local no-op/terminal -> command entry; later Plan + CommandWrites journal for sequences/partial IDs. |
| `venues/bitrue/src/trading.rs:834` | `5e1ebc068cca89013b259957b834e34abaec73947a047f535da9894eb0b5c254` | 1 | call: Trading preflight/local no-op/terminal -> command entry; later Plan + CommandWrites journal for sequences/partial IDs. |
| `venues/bitrue/src/trading.rs:1157,1226,1304,1311` | `6d2c051bb24db8acd1986f4e47c2b6dbede5aa9dfff7fb877e64ef194107bd53` | 4 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/bitrue/src/trading.rs:1130` | `afb3c3744f13da9606f5f456bd95797992a932cf227eb74b2fe4405e3c64ee98` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/bitrue/src/trading.rs:1054` | `b780b823ba70712169e1be98fecae4d2a2655f7ca35f27bea4b99f09bd684696` | 1 | call: Trading preflight/local no-op/terminal -> command entry; later Plan + CommandWrites journal for sequences/partial IDs. |
| `venues/bitrue/src/trading.rs:847` | `cb0a44751a1560cceb6c0a90e523713002ad5efbdf5418c58f0132ef2c94fce0` | 1 | call: Trading preflight/local no-op/terminal -> command entry; later Plan + CommandWrites journal for sequences/partial IDs. |
| `venues/bitrue/src/trading.rs:1055` | `dcb84cef5dcb71801d2b1b9562fcfc84b0431681917a0b6b9d2314f2e050630e` | 1 | call: Trading preflight/local no-op/terminal -> command entry; later Plan + CommandWrites journal for sequences/partial IDs. |

This table exhausts this venue’s current ratchet rows. Planning changes no source,
tests, capability flags, caller behavior, or inventory entries.

## Paired error-classification adoption proof

The error-classification owner reserves shared classifier work in #1814. Bitrue
write adoption must fix its documented P0/P1 rows through that core: native -1022
must not resend at arbitrary HTTP statuses, malformed 2xx must not become outcome
None, and partial accepted IDs must remain structured journal evidence rather than
text only (spec rows 8, 22 and 21). Preserve eligible native retries only within
reviewed shared refusal/resend rules. Add RED then GREEN proofs with the owner’s
reviewed API pin; do not encode these corrections in a venue status ladder.

## WS-only lifetime prerequisite found before source adoption

The pre-source adversarial review found a required shared-core proof beyond entry
issuance: a WS-only Read Context has no managed HTTP attempt, pending count or
delivery hold. At reviewed #1830 `2d8de248f64de168847b5cb7219713c34512b880`, later
Context issuance can prune such an expired owner before its delayed native timer
arrives. Retaining the Context in the FIFO alone then cannot authorize the original
terminal failure. The shared owner must preserve or settle this original authority.

Native controls interpose both a distinct command and a repeated numeric ID between
the first request's 5000ms deadline and timer delivery. Production adoption waits
the overseer's announced composed pin and reviewed shared retention/retirement
proof; do not fabricate HTTP, a venue authority registry, or numeric fallback.

Shared classifier #1837 is also a prerequisite for removing P2 engine-formatted
text parsing in lib.rs/account.rs during this lane’s later write adoption. Re-verify those source sites and spec rows 8/22/21
against the actual landing before the later write adoption; a local WS Context
conversion does not close them or authorize a new venue status ladder.

The shared owner confirmed the WS-only pruning gap and holds async WS adoption.
Classifier #1837 is not frozen; consume only the owner’s eventual reviewed pin,
including its expected-count/intent leg facts and Accepted-only write result seam.

## Canonical shared retention consumption for FIFO reads

The reviewed canonical #1830 pin `66cb32e89588e60bd736d3ff121f5ce52bfc4f3b`
closes the historical WS-only owner gap above in the shared 256-owner registry.
This area retains that genuine entry Context in SocketRead, uses typed terminals,
and preserves the native FIFO/timer behavior. Validation precedes send or response
consumption; expiry never grants publication or queue-advance rights. Ten WS
numeric terminals and the associated conservative correlation macro retire:
53 original occurrences become 42, lib.rs 22 becomes 11. The exact original table
remains the full-map baseline, and every remaining HTTP/private/group site stays
tracked. The public-trades dispatch no longer passes an unused numeric ID to the
native helper; private history algorithms are unchanged.
