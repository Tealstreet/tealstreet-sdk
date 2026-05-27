# Terminal Docs Drift Report (Phase 1)

- Generated: `2026-04-13T00:35:17.7983379Z`
- Scope size: `24` pages
- Priority counts: `P0=0`, `P1=2`, `P2=8`, `Monitor=14`
- Method: split-script rerank (v2 similarity + stale phrasing + route/asset checks; draft ignored by default).

## Ranked Pages

| Priority | Score | Doc | Primary Issue | Suggested Action |
|---|---:|---|---|---|
| P1 | 6 | `apps/documentation/docs/desktopclient/windows.mdx` | `version_drift` | `surgical_edit` |
| P1 | 6 | `apps/documentation/docs/FAQ.mdx` | `link_drift` | `surgical_edit` |
| P2 | 4 | `apps/documentation/docs/desktopclient/linux.mdx` | `none_detected` | `cleanup_review` |
| P2 | 3 | `apps/documentation/docs/desktopclient/mac.mdx` | `none_detected` | `cleanup_review` |
| P2 | 3 | `apps/documentation/docs/trade/modules/aggr.mdx` | `none_detected` | `cleanup_review` |
| P2 | 3 | `apps/documentation/docs/trade/modules/econ-events.mdx` | `none_detected` | `cleanup_review` |
| P2 | 3 | `apps/documentation/docs/trade/modules/fee-calculator.mdx` | `none_detected` | `cleanup_review` |
| P2 | 3 | `apps/documentation/docs/trade/modules/forex-calendar.mdx` | `none_detected` | `cleanup_review` |
| P2 | 3 | `apps/documentation/docs/trade/modules/open-interest-chart.mdx` | `none_detected` | `cleanup_review` |
| P2 | 3 | `apps/documentation/docs/trade/modules/wsot-leaderboard.mdx` | `none_detected` | `cleanup_review` |
| Monitor | 2 | `apps/documentation/docs/trade/modules/basic-chart.mdx` | `none_detected` | `keep` |
| Monitor | 2 | `apps/documentation/docs/trade/modules/browser-view.mdx` | `none_detected` | `keep` |
| Monitor | 2 | `apps/documentation/docs/trade/modules/chaser-orders.mdx` | `none_detected` | `keep` |
| Monitor | 2 | `apps/documentation/docs/trade/modules/cli.mdx` | `none_detected` | `keep` |
| Monitor | 2 | `apps/documentation/docs/trade/modules/cross-trades.mdx` | `none_detected` | `keep` |
| Monitor | 2 | `apps/documentation/docs/trade/modules/custom-module.mdx` | `none_detected` | `keep` |
| Monitor | 2 | `apps/documentation/docs/trade/modules/dom.mdx` | `none_detected` | `keep` |
| Monitor | 2 | `apps/documentation/docs/trade/modules/news-feed.mdx` | `none_detected` | `keep` |
| Monitor | 2 | `apps/documentation/docs/trade/modules/position-history.mdx` | `none_detected` | `keep` |
| Monitor | 2 | `apps/documentation/docs/trade/modules/screener.mdx` | `none_detected` | `keep` |
| Monitor | 2 | `apps/documentation/docs/trade/modules/ticker-data.mdx` | `none_detected` | `keep` |
| Monitor | 2 | `apps/documentation/docs/trade/modules/ticks-chart.mdx` | `none_detected` | `keep` |
| Monitor | 2 | `apps/documentation/docs/trade/modules/twitch.mdx` | `none_detected` | `keep` |
| Monitor | 2 | `apps/documentation/docs/trade/modules/twitter.mdx` | `none_detected` | `keep` |

## Top 10 Investigation Targets

- `apps/documentation/docs/desktopclient/windows.mdx` -> score `6` (P1), notes `lastUpdatedDays=0; v2Similarity=0.816; draft=false`
- `apps/documentation/docs/FAQ.mdx` -> score `6` (P1), notes `lastUpdatedDays=0; v2Similarity=0.63; draft=false`
- `apps/documentation/docs/desktopclient/linux.mdx` -> score `4` (P2), notes `lastUpdatedDays=0; v2Similarity=0.601; draft=false`
- `apps/documentation/docs/desktopclient/mac.mdx` -> score `3` (P2), notes `lastUpdatedDays=0; v2Similarity=0.46; draft=false`
- `apps/documentation/docs/trade/modules/aggr.mdx` -> score `3` (P2), notes `lastUpdatedDays=0; v2Similarity=0.506; draft=false`
- `apps/documentation/docs/trade/modules/econ-events.mdx` -> score `3` (P2), notes `lastUpdatedDays=0; v2Similarity=0.547; draft=true`
- `apps/documentation/docs/trade/modules/fee-calculator.mdx` -> score `3` (P2), notes `lastUpdatedDays=0; v2Similarity=0.519; draft=true`
- `apps/documentation/docs/trade/modules/forex-calendar.mdx` -> score `3` (P2), notes `lastUpdatedDays=0; v2Similarity=0.539; draft=true`
- `apps/documentation/docs/trade/modules/open-interest-chart.mdx` -> score `3` (P2), notes `lastUpdatedDays=0; v2Similarity=0.557; draft=true`
- `apps/documentation/docs/trade/modules/wsot-leaderboard.mdx` -> score `3` (P2), notes `lastUpdatedDays=0; v2Similarity=0.586; draft=true`

## Interpretation Notes

- Generated from the split PowerShell audit scripts.
- High V2 similarity usually indicates carry-over pages that need deeper modernization.
- Use this ranking for triage; keep final doc decisions manual.
