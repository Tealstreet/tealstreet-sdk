# KuCoin Futures stop-limit edits

Rust KuCoin Futures keeps a standalone stop's trigger in `Order.price` and its
native limit execution price in `Order.extra.conditionalOrderPrice`. REST and
private order rows retain both; a sparse stop event keeps the previously known
execution price. An edit moves the trigger while retaining the execution price,
limit type, native lots, time in force, trigger source and explicit close flag.
A definitive replacement refusal puts back the original type, execution price,
trigger and original native lots once. An edit of a native `closeOrder: true` remains a
full-position close even when the venue row reports a nonzero size; its request
omits size. New placements keep the house sizing contract: an explicit nonzero
amount stays sized even with `close: true`. New SL/TP orders remain stop-market,
including the app convention `triggerPrice == price`; only proven native
execution-price metadata on an edit or put-back selects stop-limit. A full-position size edit is refused before cancellation. Invalid
or sub-tick limit prices also refuse before cancellation.

KuCoin documents limit and market as independent from the stop direction and
trigger, and requires price for limit orders. Its closeOrder flag grants the
full-position operation independently of the reported size. Sources:

- [Classic Futures Add Order](https://www.kucoin.com/docs-new/rest/futures-trading/orders/add-order)
- [Official request schema, pinned](https://github.com/Kucoin/kucoin-universal-sdk/blob/c2f39bdb8b7d5f31cfb3fd532f9baf2f09fd8f12/spec/rest/api/openapi-futures-order.json)

This intentionally diverges from the existing TypeScript KuCoin implementation,
which sends these stops as market orders and derives closeOrder from zero size.
No TypeScript trading code changes here.

This is a live wire-format repair. KuCoin Futures trading still dispatches
legacy signed `Action::Http`; its existing send-time stop guard is not a managed
Prepare/Claim veto, and it does not use `present_plan_failure`. The separately
assigned guarded-edit Plan cutover must establish those final-Claim guarantees.
Existing cancellation proof, ambiguous-outcome handling and one-shot restoration
rules are retained by this repair, without claiming that cutover is complete.
