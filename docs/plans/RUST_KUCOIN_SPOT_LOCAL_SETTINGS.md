# KuCoin Spot fixed-setting completion

Base: actual `f8163b7b852`; shared #1989 is actual `ce278e0b4a9`.
Owner: codex-9czdjh. One venue area; no signer, engine or socket changes.

## Native contract and exact move

`spot/trading.rs` has two lexical `cx.done(command, "[]".into())`
sites covering three commands, all before credentials:

- SetLeverage ignores the requested value and calls Store::update_setting with
  leverage 1 and buy/sell Some(1). Preserve that method exactly: it creates a
  default row for an unknown symbol. Margin type and other fields stay intact.
- SetMarginType and SetPositionMode are successful no-ops, for both boolean
  values and unknown symbols. Preserve the successful Spot capability policy.
- Keyless, public-only and keyed configurations behave alike; no HTTP, provider,
  timer, Context or journal is manufactured.

Call `cx.done_local_setting("[]".into(), Store-only closure)` directly in each
branch; return after Ok, Rejected or Revoked. The shared engine is the sole
entry/receipt/capacity/projection/terminal authority. No local helper or fallback.
The deliberate caller change is refusal before Store mutation/success when the
receipt is missing/expired/sub-ms, an owner is duplicated, or capacity is full.
Rejected already emitted its typed Mutation/NotSent failure. Revoked grants no
projection or second terminal. Preserve original owners on duplicate ids.

## Frozen inventory and collisions

| Source | Fingerprints | Occurrences |
| --- | ---: | ---: |
| `venues/kucoin/src/account.rs` | 3 | 3 |
| `venues/kucoin/src/funding.rs` | 1 | 1 |
| `venues/kucoin/src/history.rs` | 5 | 5 |
| `venues/kucoin/src/identity.rs` | 2 | 2 |
| `venues/kucoin/src/lib.rs` | 7 | 9 |
| `venues/kucoin/src/secondary_tests.rs` | 9 | 10 |
| `venues/kucoin/src/spot/mod.rs` | 10 | 11 |
| `venues/kucoin/src/spot/trading.rs` | 9 | 10 |
| `venues/kucoin/src/trading.rs` | 5 | 5 |

The exact Spot trading row
`2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b: 2`
is removed, nothing else. Venue 56 -> 54 occurrences, 51 -> 50 fingerprints;
Spot trading 10 -> 8 occurrences, 9 -> 8 fingerprints. All remaining signed,
history, account and socket sites stay with their existing owners/plans.
The full frozen source-to-engine map remains RUST_KUCOIN_REQUEST_CONTEXT.md;
this slice adopts only the two success sites above.

Open-PR map checked before edits. Own #1986 touches the same trading file in
separate refusal hunks; keep its frozen head unchanged, integrate whichever
lands first. No other lane claims this file. Signer branch remains plan/RED-only.

## Phases and evidence

1. Commit this plan and independently review native/API/collision fit.
2. Commit compiling original assertion REDs against unchanged native source:
   all three commands refuse command-owner capacity and sub-ms/expired receipts before projection;
   duplicate id preserves its original live owner and Store. Working controls
   pin exact [] plus native fields/no-ops/unknown symbols, keyless/public-only/
   keyed precedence and zero HTTP/signing. Capacity comes from 256 genuinely admitted command owners, each in its own
   callback. A working control pins that 256 Background Contexts do not block this
   Context-free seam. Engine::handle now supplies a default receipt (#2004), so
   missing-receipt defense remains shared-engine coverage, not a native receipt.
   Sub-ms/expired controls advance the exposed callback clock after genuine
   receipt ingress; they do not create a Context or alter the receipt.
3. Replace the two branches, prune only that fingerprint, add CALLERS/AUDITS/
   QUALITY, rerun controls and scoped KuCoin/Binance/strict/fmt and both ratchets.
4. Independent source/adversarial, caller and full-map reviews, draft PR and full
   CI. No merge without a window. Architecture: shared #1989 owns the concern;
   the venue only supplies native Store projection/result.

## Executed consumer evidence

Original tests committed at `b16e7262fdd` against unchanged venue source:
six assertion REDs (capacity and sub-ms receipt for each command), four working
controls. The duplicate is already intercepted by Engine and is explicitly a
GREEN diagnostic/original-owner control, not a new migration failure. Final
source makes all ten tests GREEN with identical assertions. Missing-receipt and
managed internal resource refusal remain shared-engine defense, not a fabricated
native receipt. No credential, Context or HTTP is created by this adoption.

## Actual refusal-slice integration

Actual #1986 `45548167038` and #1993 `5f86ae51717` are genuinely integrated.
The remaining KuCoin inventory is now 48 -> 46 occurrences, 43 -> 42 fingerprints;
this slice still removes exactly the same count-two success row. Incoming
refusal source/tests and all unrelated inventory entries are retained. Original
LocalSetting controls remain byte-identical to `b16e7262fdd`.
