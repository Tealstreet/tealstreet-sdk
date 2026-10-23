# Bitrue local history Read authority

Base actual `2badd1cb0e3217b844e73d6b81ec023b3abc6d8b`; plan before tests/source.
Refresh open PR file lists before edits. Only `venues/bitrue/src/history.rs` and
new disjoint `tests/local_history_context.rs` source/test paths are proposed.
Private HMAC ownership is notified; no lib/account/trading/signing helper moves.
Bitunix socket/account/lib files remain #1771's, BloFin history remains #1865's.

## Exact local slice

Five synchronous no-I/O numeric terminals in history_query become direct shared
initial Read admission plus original-root Context completion: unsupported complete
order history, unsupported income ledger, absent credentials for fills, invalid
fill range and unknown/missing fill symbol. Current Bitrue inventory42 occurrences
in27 hash rows/five files becomes37 occurrences/22 rows/four files. Remove only
all five existing history.rs hashes, preserving other incoming venue/file maps.
Legacy private fill requests, HistoryRows/Completion, signing/retries and account
lookup stay held; WS PublicTrades delegates to its already managed FIFO path.
No history bridge, private managed-read or whole-venue zero claim.

Use a pure local Read policy declaration: credential None, timeout20,000ms, no
pacing, Never retry. Only returning branches admit; successful private fill/lookup
or WS delegates acquire no extra owner. Rejected/Revoked return without numeric
fallback. Each branch consumes fail_context directly; no local finalizer, owner
registry, fabricated HTTP, signer or route. These branches read no remote private
data. Preserve every native message and precedence: unsupported kind checks,
then authentication, then negative/nonfinite/reversed range, then resolve_symbol.
Unsupported history kinds gain Unsupported/NotSent. Local input/absent credential
errors gain InvalidInput/NotSent. Shared admission errors retain engine-owned
kind/reason, outcome None, one terminal/no companion or history marker.

## Original-RED and compatibility proof

Commit tests first. Engine<Probe> delegates the native guard_pending_placements
and guard_order_mutations plus all touched callbacks. Seed the same market shape
as existing history tests; no startup/network needed. Use genuine Query callbacks
for all five local exits. A prior genuine no-ID callback allocates all256 bounded
Background roots; assert real capacity, repeat local denials, preserve peers,
explicitly retire roots and recover at one clock. Exercise invalid raw clocks and
fixed-clock owner reuse beyond256; verify exact one terminal/no opaque work.

Original assertion REDs pin typed local metadata, shared initial capacity refusal
and invalid-clock refusal. Compatibility GREENs pin messages and validation order,
zero-I/O/Store stability and unchanged valid signed fill request/reply. Preserve
range units/flooring, symbol aliases, default/clamped limit, IDs, signed request
headers/routes/body and native row codec. Existing complete-history/income
capabilities remain unsupported. No new retry or local classification table.

Plan, source/adversarial, caller and full-map reviews must be CLEAN. Run new tests
on original source, freeze assertions, adopt, then require all GREEN. Run Bitrue
plus Binance native tests, scoped strict Clippy/fmt, bans and both request-context
checks. Append CALLERS/AUDITS/QUALITY and README, prune only the five exact hashes.
CI owns WASM/replay/size/timing. Explicit train slot, latest landing integration,
fresh separate guard and head-matched merge remain mandatory. Architecture gate:
shared engine admission/completion only; no venue-specific cross-cutting mechanism.

## Exact selected fingerprints

| Hash (count one each) | Original terminal |
| --- | --- |
| `c26319aae17ffaa69a204fb71bea1ad4e83a1f01411ca808627909808f6be6ef` | `cx . fail ( command , "Bitrue v2 has only single-order lookup, not complete order history" )` |
| `3cdc4e2ec5596e8f05582d9ea14e49f5e810d97c769e5189330ab4292ee9b9b4` | `cx . fail ( command , "Bitrue v2 transfer and force-order histories are not a complete PnL/funding ledger" )` |
| `3222c9dc7146bd368025519801a954c755cb187050b8dcca08eea2163dfb87f8` | `cx . fail ( command , "Bitrue fills require authenticated futures credentials" , )` |
| `b0723bf50b38aec58f93f9e0c3c86eb7adb6b09590d7255838d094a9bd119c37` | `cx . fail ( command , "Invalid Bitrue fill time window" )` |
| `1d929e91360e3f2b465b4f60d8024a7aa9f9167e8573e49457119fe4d06f634d` | `cx . fail ( command , "Bitrue fills require a known symbol" )` |

## Executed phase receipt

Tests-first `23c35ea2059` runs unchanged production with five assertion REDs and
three compatibility GREENs. The same frozen eight controls pass after adoption.
All461 Bitrue/Binance native tests across38 targets, scoped strict Clippy and fmt
pass. Three independent source/adversarial, caller and full-map reviews are
CLEAN. A newer landing ratchet prune must be genuinely integrated before final
bans/request-context verification and publication; no old inventory is restored.
