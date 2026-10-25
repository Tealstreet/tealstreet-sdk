# Paradex config and initial local terminals

Base: actual landing `77f124089477a6aed502d9b02aa7f08752b7bfa5`.
Sam's signing decision is one model for every venue: hosted mode supplies the
minimum signing material to the trusted provider; local and remote signing stay
available. Paradex B/C is retired. This slice changes no signer material or JWT
custody.

## Seven-site boundary

The actual scanner has 48 remaining Paradex occurrences. Remove exactly seven:

| Path / native branch | Occurrences | Authority |
| --- | ---: | --- |
| account.rs / fetch_config GET /system/config | 1 | unsigned Background Read, 20s, Never |
| trading.rs / initial unsupported command | 1 | genuine initial Read or Mutation by command intent, fail_context |
| trading.rs / isolated margin and hedged mode refusals | 2 | genuine initial Mutation, fail_context |
| trading.rs / fixed cross-margin and one-way empty successes | 2 | existing done_local_setting, no Context or journal |
| trading.rs / initial missing private account | 1 | genuine initial Read for FetchLeverage, otherwise Mutation, fail_context |

Expected inventory: account 10->9, trading 21->15, whole family 48->41.
Keep all other keys and every remote trading/history/JWT/signing path unchanged.
No staging row, shared engine edit, optional signer carrier or fabricated credential.

## Native behavior and entry transaction

Keep current validation order, exact native messages, Unsupported classification,
NotSent on all native refusals, including the existing Read refusals. New shared
admission refusals keep the engine projection (Read outcome None, Mutation
NotSent); this does not relabel a native NotSent as write evidence.
The unsupported query subset is Read; unsupported controls/cancellation are
Mutation. Missing-account FetchLeverage is Read; other missing-account commands
are Mutation. Determine operation from the matched command before it is moved,
without selecting or changing any signer.

Only enter the shared command transaction on the selected initial terminal
branch. Shared clock/capacity/duplicate rejection owns its terminal, with no
second native failure or Store change. Fixed no-op successes call the landed
LocalSetting entry with an empty Store projection and result `[]`; never create
an empty successful write journal. Existing genuine caller receipt expiry is
honored by LocalSetting. Successful remote fallthrough remains native.

## Config lifecycle

Replace the raw config request with one real Background Read root and managed
Prepare/Bind/Claim. Retain the root in Request::Config and validate exact live
read delivery BEFORE HTTP status, JSON decode, credential installation, socket
connect, setting publication or auth/snapshot scheduling. Retire the root after
synchronous validated native publication and on every failed/preparation/timeout path. Keep
GET URL, empty headers/body, mainnet/testnet selection and native 2s retry.
Each retry is a new attempt with a fresh root after retirement; no engine retry
or venue expiry timer. Admission failure follows the same native retry path.
No fake Context may stand in for a refused admission.

The 20s config bound now includes host preparation/claim delay. An early,
expired, duplicate or revoked config response must not install credentials or
trigger private I/O. This is the only new expiry boundary in this slice.

## Original controls and execution order

1. Commit this plan and the separate engine-gap assignment note first.
2. Add a dedicated integration target on unchanged production; compile and run
   all original assertions. Record intended failures separately from controls.
3. Native controls cover exact six local results and validation precedence on
   both networks, no Store/I/O effects, and exact unsigned config bytes.
4. Entry REDs cover genuine retained peer saturation/recovery, invalid clock and
   duplicate owner for all six local branches, including Read/Mutation outcome.
   LocalSetting saturation uses retained command owners (not only Contexts).
5. Config REDs cover Prepare before outbound, parked-provider/preparation expiry,
   no install from early or late response, preparation refusal, native retry and
   duplicate DATA. Drive real Prepare/Bind/Claim, no hand-built handles.
6. Implement, preserving original assertions. Run focused Paradex/Binance tests,
   strict touched-crate Clippy, fmt and exact request/socket ratchets. Request
   independent native/source and authority review, then draft PR and Rust CI.
   One watcher >=120s; READY only after exact-head required CI succeeds.

Shared docs may append; manifest pruning touches only account.rs/trading.rs.
The seven-site source scope includes only Config representation/issuance and
callback guard/failure cleanup plus those six initial trading branches.

## Original receipt and implementation checkpoint

Plan `b29ea97d0d4`, metadata clarification `882575cc101`, original test-only
checkpoint `e743120ab7b`. Production and manifest were byte-exact to actual
77f124 when the originals ran: 22 compiling assertion REDs, 4 working controls,
zero ignored. Capacity/clock assertions execute separately for all eight native
Read/Mutation presentations of the six local branches. Original config REDs
stop first at eager raw HTTP; their deeper claim/recovery assertions are only
executed after that boundary turns green. No compilation error is a RED.

All original 26 controls now pass unchanged. The implementation uses the
existing Background-specific delivery validator; the command Read validator
correctly rejects a Background owner and was corrected during implementation.
No new engine authority or signer capability was introduced.

The historical snapshot-rebind checker control assumed the live account source
would remain at ten sites forever. Checker owner claude-m26ps4 independently
CLEARED the narrow correction: pin the historical after-map to immutable actual
77f124 and additionally require the live map be its subset via unchanged check().
Both historical 10/10 hashes and every regrowth negative remain. The production
checker and bootstrap rows are unchanged; this is not an inventory exception.

Authority review claude-fe7lgm and native/source review codex-9czdjh CLEAR at
551a79938cf, with cleanup-order follow-up8592354fb5c CLEAR from both. Successful
config publication now precedes its single retirement. Checker-owner review
claude-m26ps4 CLEAR for f193dd5d055's test-only historical pin and live subset.
Existing history/startup fixtures now settle startup config or drive genuine
managed preparation; every existing assertion and both original test blobs stay.
