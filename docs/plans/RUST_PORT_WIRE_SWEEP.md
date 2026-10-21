# Port-review and wire-contract follow-up sweep

Start from landing `687fe0c1e08925a774c64e8bbb736f56052af206`.
Scope: still-open findings under Port reviews and Wire contract in
`RUST_AUDITS.md`; production impact governs each disposition.

1. Reverify every open or partly open finding against current source, existing
   regressions and landing history. Record stale findings with the responsible
   merged PR/commit. Collect those strikes in one documentation PR.
2. For remaining production defects, use fresh landing branches and one batch
   per venue or shared component. Reproduce each defect with a regression before
   repairing it; preserve intentional behavior and nearby caller documentation.
3. Independently review each batch, run focused venue plus Binance gates,
   formatting, strict Clippy and bans, and relevant SDK tests/typecheck/lint.
   Push for full CI, integrate landing as required, then run the exact scope
   guard separately before merging on green. Report every merge to the overseer.

Do not change owner decisions, infer undocumented live behavior, invent financial
data or silently discard incomplete history. List owner-decision and live-capture
dependencies explicitly for the overseer. Do not change budgets or run local
release WASM, full replay sweeps, timing gates or load tests. Use four Cargo jobs.


## Revalidation receipt (2026-10-21)

Source and existing regressions inspected at `687fe0c1e08`; this documentation
pass does not claim new venue test executions or authenticated live evidence.
The inline audit strikes name actual merged PRs/ancestor commits. Wire repairs
are complete (#1345 and #1527); remaining historical CI notes are a diary.
Bridge command/account/wire repairs are evidenced by #1375/#1392/#1430/#1518.
Registration polling/credential feedback is evidenced by #1392/#1487/#1611.
Public startup repairs are evidenced by #1356/#1516; private keyed-row repairs
by #1459/#1493/#1517/#1524/#1606. An alias inventory is not proof of every
option's behavior, and generic diagnostics are not typed credential evidence.

Confirmed implementation batches, each requiring a fresh landing branch,
reproducing regression and independent review before landing:

| Batch | Remaining production defect |
| --- | --- |
| BingX | Sequential placement failure loses earlier acknowledged IDs from typed failure; preserve native error and unknown-outcome policy. |
| Bitunix | Public row admission/private startup, reconnect backoff, private keyed-row retention and WS-ID-triggered read starvation, explicit history failure/completeness, coded/throttled visible account-read failures, secondary GET signing plus egress, host clock, forced route and stale-book invalidation. Preserve write-revision fences and prerequisites. |
| Bybit | Ordinary keyed REST orders/positions and structural WS orders/positions can starve valid peers; preserve Hyro handshake and execution completeness. Two owned history markers. |
| Bitget | Structural WS order/position row admission; REST keyed retention already repaired. Keep financial fills strict. |
| BloFin / Hyperliquid / OKX / Paradex / WEEX | Query-associated HISTORY_INCOMPLETE ownership: 1/1/3/1/5 sites respectively; Bitunix and Bybit account for the other three. Attribute the real command before completion, preserving result/error policy. |

No production defect is established by cache-only `removeOrders`, caller-owned
post-validation engine lifetime, optional candle `lastSide` without a mirrored
consumer, or an all-market feed ignoring narrower ticker demand. KuCoin's
explicit leverage setter is already a string; numeric placement leverage is
not evidence of rejection without an endpoint contract.

## Owner, capture and protocol dependencies retained

These are exclusions from routine repairs, not claims that an uncertain path
was tested live. Existing decisions remain in force.

| Family / issue | Evidence or decision needed before changing behavior |
| --- | --- |
| Bitunix signature resend / same-socket terminal auth | Endpoint-specific signature/rejection and login-control evidence; do not infer permanent WS refusal from HTTP codes. |
| Bitunix null success, floating leverage, protective ACK identity | Captured endpoint response/lexical/identity contract; do not accept unidentified protection or round values. |
| Bitunix protective sibling scope / TP-SL side / first-session cumulative fills | Explicit native leg/scope and execution-baseline policy; no guessed hedge side or fabricated historical fill. |
| Bitunix silent private sessions | Authenticated watchdog capture; retain explicit legacy first-pong compatibility. |
| Bitunix cancel/amend finality and caller protective IDs | Preserve proven terminal state and one-native-order identity; read-only convergence needs its own policy. |
| Binance batch closePosition / PM -2011 | Existing replacement decision; endpoint/ownership capture before treating an ambiguous PM refusal as disappearance. |
| Binance COIN-M EXPIRED / CALCULATED | Triggered-order and liquidation packets before weakening terminal or execution identity rules. |
| Bybit split preflight / rate timing | Owner choice for atomicity and native timing, rather than automatic retries of writes. |
| Kraken income over 500 | Live cursor probe from RUST_LIVE_GATE; newest-page partial contract remains explicit. |
| KuCoin numeric placement leverage | Native endpoint rejection/lexical contract; existing polling cadence is accepted. |
| Phemex unknown-ID cancel / additional lifecycle read | Preserve explicit identity proof; existing read lifecycle is accepted. |
| WEEX Index triggers / missing income asset / empty fill identity | Native reference, asset and execution identity evidence; do not substitute Mark, assume USDT or fabricate an ID. |
| MEXC and Hyperliquid signed fill fees | Verified debit/rebate convention before changing sign; absolute values can convert rebates to charges. |
| Coinbase browser signing | Existing owner decision 2 and payload-signing migration boundary; do not restore secret-bearing compatibility implicitly. |
| Coinbase CFM equity/uPnL | Authoritative accounting evidence and owner decision; no invented aggregation. |
| WOO attached trigger fallback / unknown STOP | Reliable per-leg trigger reference and direction; no guessed protective kind. |
| Aster / Bitrue / Paradex financial rows | Existing strict readiness/completeness contract; never invent money or silently discard financial execution rows. |
| ApeX protective caller IDs / cached leverage suppression | Native per-leg identity and explicit-write policy; retain current refusals/writes. Private proxy lifecycle still needs capture. |
| Paradex zero TP/SL | Existing protection-intent policy; never drop requested legs to place an unprotected entry. |
| GRVT asynchronous amendment | Live cancellation timing and owner choice for bounded read-only confirmation; never replace on ACK alone. |
| Polymarket typed auth / user-WS refusal | Deliberately deferred classifier; endpoint-specific signed evidence and captured refusal shape. Development-only boundary remains. |
| Bitunix / Lighter / TxFlow background completeness markers | Six emitters have no query owner. An account-scoped coverage protocol needs a separate decision; never fabricate a command or broadcast another client's semantic marker. |

Accepted product/representation boundaries are also preserved: retired Backpack
spot, disabled BreakoutProp, Kraken omitted-zero funding/unsupported 3m candles,
Bitget UTA combined-leg identities, and Polymarket proof for every cancellation.
