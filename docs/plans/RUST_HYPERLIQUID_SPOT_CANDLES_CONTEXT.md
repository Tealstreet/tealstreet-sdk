# Hyperliquid spot candle custody

Plan-first base: actual `77f124089477a6aed502d9b02aa7f08752b7bfa5`.
Scope is the standalone spot deployment and spot markets inside the merged
perps deployment, on production and testnet. Hosted/local/remote signing share
Sam's one model, but this public Read never selects a signer or a credential.

## Exact inventory and ownership

The actual request checker counts lib.rs 7 fingerprints /9 occurrences and
Hyperliquid37 occurrences overall. Remove six real legacy terminal occurrences:
- FetchCandles unsupported interval, missing market, invalid time range: three;
- Pending::Candles response success and serialization failure: two;
- Pending::Candles failed callback: one.

Expected lib.rs9->3 and family37->31. Remaining lib sites are the shared info
sender and two conservative Builder matches. Keep them exact; no rename prune.
No other manifest key, socket inventory, shared engine, crypto or Cargo change.

codex-5c57jl explicitly released the narrow candle hunks and tests/spot.rs plus
request_context_candles.rs: its whole socket adoption already landed, no local
hold. Preserve Connection/interests, mode_refresh_due, SocketRefresh and
on_socket_interests_released. Preserve current History response/delivery/root
retirement and account FIFO, Builder, signing, trading, subscriptions and
catalogs. The history owner was notified to confirm there is no unpublished
candle overlap. Do not edit history-specific arms to implement this slice.

## Existing API and native behavior

Reuse the current perps ManagedCandles path for every FetchCandles deployment,
without a second wrapper or engine API. Admit a genuine credential-free Read
Context (20s, Never) at initial command entry, before any native refusal or
outbound. Keep interval -> market -> range validation order and exact native
messages. A shared admission refusal owns one typed terminal, with no financial
outcome. Duplicate/stale entry never consumes the original command owner.

Keep native POST /info, exact Content-Type: application/json, no signer headers,
exact candleSnapshot body including native @coin, interval, computed limits and
bounds. Native row/schema/validity behavior remains. Each command has exactly
one physical request; no Plan, retry, JWT, Session, queue or venue timer.
The existing managed response/item/failure paths validate their root and exact
physical callback before staging or completion. Remove the obsolete Candles
pending payload and its legacy branches, preserving the current perps path.

The20s lifecycle includes host preparation/signing-independent claim delay.
Unreleased, expired or duplicate HTTP must neither stage candle rows nor emit
DATA. Preparation/provider errors keep the existing perps typed metadata, including
NotSent for an unreleased preparation failure or timeout; a released timeout has
no financial outcome. No later
transport error can be relabeled as a successful empty response.

## Original RED checkpoint

Before source, add only tests/request_context_spot_candles.rs. Compile and
execute original runtime assertions on the actual source, with no missing API
counted as RED. Cover both networks and standalone/merged spot:
- working exact native wire/schema and interval/market/range precedence;
- intended Prepare-before-outbound assertion;
- early/unreleased response and expired prepared/released request denial;
- genuine shared capacity and invalid-clock refusal, then recovery;
- duplicate command custody and duplicate response;
- typed preparation/transport failure with no DATA or financial outcome.
Keep working perps and socket/history controls. Report first failing assertions
honestly where later suffix checks cannot run on legacy source.

The existing perps test named merged_spot_candles_remain_on_the_existing_legacy_path
is a historical hold control. Only during production adoption, replace its
transport expectation with actual managed Prepare/Bind/Claim while preserving
its exact spot wire/result and following perps assertions. Existing spot native fixtures use a raw drain for legacy transport. Adapt only
the two candle drivers to real prepared_actions, preserving every native
wire/financial assertion.

Production follows a reviewed plan and compiling original RED commit, then
unchanged original assertions, native Hyperliquid/Binance checks, strict scoped
Clippy/fmt and full request/socket ratchets. Independent source/native review,
exact-head Rust CI, one120s watcher and an assigned venue window are separate
gates. No source implementation is included in the plan/RED checkpoint.

## Original runtime receipt

Plan89a67dc915a, original test-only5012318fc91 on unchanged actual77f124 source
and inventory: 11 compile/run, two native/duplicate working controls PASS and
nine intended assertion REDs, zero ignored. Four Prepare controls execute every
standalone/merged and mainnet/testnet variant; early HTTP actually published DATA
on legacy source. Capacity and clock REDs show native local validation bypass.
Deadline/preparation/recovery suffixes originally stop at the Prepare assertion.

Two previously gated suffix expectations incorrectly assumed no outcome on
unreleased failures. The existing perps controls independently pin NotSent on
preparation refusal and unreleased timeout, versus None after released timeout.
Correct only those new suffix expectations to the reused seam's actual contract;
retain every original RED boundary assertion. No engine/metadata behavior change.
The incidental unused legacy command binding/import-format setup errors during
implementation were corrected and are not counted as original RED evidence.


## Production checkpoint

All FetchCandles entries now reuse the existing ManagedCandles implementation.
The obsolete spot pending payload and its response/item/failure arms are removed.
The generic info sender, Builder/History and socket callbacks are unchanged.
The manifest changes only lib.rs, deleting the six audited occurrences without
adding a fingerprint: lib9->3 and Hyperliquid37->31.

The eleven dedicated controls, ten existing candle controls and43 spot controls
pass (64 total, zero ignored). The two raw spot fixture drains and the historical
legacy-path expectation now drive actual Prepare/Bind/Claim; native wire/result
assertions remain. The only original-test correction is the disclosed gated
unreleased-failure metadata expectation above. Production review and CI remain
separate gates.

Full bans and the15 request/8 socket checker controls pass, and scoped rustfmt
passes. The broader Hyperliquid/Binance native run and all-feature Clippy were
stopped under Sam's machine-load directive; they are not passing receipts.
CI covers the broader validation. Future local builds are one scoped run at a
time with CARGO_BUILD_JOBS=4 until that directive is lifted.

Production review restored full-batch preparation-failure inspection. The first
restored blanket ManagedHttp prohibition failed10/11 because shared retirement
correctly emits Cancel. The guard now requires exactly that preparation's one
Cancel (no signing token), and rejects raw HTTP and every Prepare/Bind action.
This discloses the second previously-gated suffix correction; no runtime change.
