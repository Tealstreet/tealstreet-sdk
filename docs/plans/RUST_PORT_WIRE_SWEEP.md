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

The implementation batches identified above have landed. The closing receipt
rechecked each actual merge's parents/reviewed head, tree and landing ancestry;
recorded own-file scopes also match. These are source/regression receipts, not
new authenticated live captures.

| Batch | Actual merged repair evidence |
| --- | --- |
| Stale audit strikes | #1619 (`1ad3a79fb28`). |
| BingX acknowledged placement IDs | #1617 (`4e2f92c2622`), preserving native failure/unknown-outcome policy. |
| Bybit keyed REST/WS peers and leverage ACK fence | #1620 (`4592d985e8a`), #1621 (`7adb43c4beb`); Hyro strict handshake remains. |
| Bitget structural WS peers | #1625 (`02a0db19f3c`), preserving identity/mode and native financial fields. |
| Fourteen query-associated completeness markers | #1627 (`9a444620376`), #1628 (`d659c18c3f5`), #1629 (`a3c4e827722`), #1630 (`b2092b50aca`), #1631 (`e9049c794c8`), #1632 (`41cd4914a70`), #1633 (`a29812d8e49`). Warn semantics and caller isolation remain; Bitunix additionally rejects failed history reads instead of returning truncated success. |
| Bitunix public startup, socket/book lifecycle, secondary GET and private rows/history | #1636 (`62adf7651b0`), #1639 (`77a5e5faaec`), #1640 (`93bb218ffda`), #1641 (`f156e3dc0aa`), #1643 (`163929bc0f5`), #1645 (`6642e51468b`). |
| Bitunix clock, login controls and forced route | #1648 (`205ceb3fe8d`), #1649 (`fc5ab92248a`), #1650 (`b2059cce998`). |
| Bitunix scheduling/keyed reads and scoped account recovery/balance progress | #1655 (`ba3d5bc69ee`), #1658 (`67e81cfc13b`), #1662 (`501fb664edb`), #1665 (`8f7b0cee3ff`). Write/prerequisite fences and no-write-retry policy remain. |

The final remaining-defect recheck found two additional routine repairs.
WEEX empty-fill-identity visibility landed in #1670 (`ce906e1368e`): valid fills
remain available while unidentified rows are skipped with one owned incomplete
marker and coded missing-ID warning per query. No identity is invented.
Bitunix per-symbol settings progress landed in #1668 (`7fd70c13e34`): ordinary
metadata/auth/backoff uses its symbol-attempt serial and accepted settings-write
revision, so routing-ID notifications cannot starve it. Accepted settings writes
still fence older metadata; command-owned prerequisites keep their generation
fence. Both final repairs are included in the 27-merge receipt.

No additional non-owner/non-live production defect remained in this Port reviews
and Wire contract scope after the source/regression recheck. The exclusions below
remain open; this is not a cutover-readiness or authenticated-live claim.

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
| Bitunix signature resend / same-socket terminal auth | Endpoint-specific signature/rejection and terminal login-refusal evidence; do not infer permanent WS refusal from HTTP codes. Malformed login controls/retry ownership are already repaired by #1649. |
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
| WEEX Index triggers / missing income asset | Native reference and asset evidence; do not substitute Mark or assume USDT. Empty fill identity visibility is repaired by #1670 without inventing an ID. |
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
