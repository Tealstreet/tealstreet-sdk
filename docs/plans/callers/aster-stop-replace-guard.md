# Aster conditional replacement safety

Aster V1 and V3 share the conditional amend job in `venues/asterdex/src/trading.rs`.
It cancels the native order before submitting the replacement. The former optional
planning check skipped absent prices and accepted arbitrarily old observations;
a native refusal after cancellation ended the job without a restore.

## Admission

Before planning and again in the engine's final `http_release_veto` claim, require
an unchanged original order and a positive reference observed within 5 seconds,
without a future receipt timestamp. Compare the rounded native replacement trigger
against its actual working type: `MARK_PRICE` uses mark; `CONTRACT_PRICE` uses last.
Size-only amendments use the original trigger and undergo the same check. Missing,
stale or wrong-side evidence refuses with typed NotSent and leaves the order open.
The final veto is synchronous and read-only; normal engine claim cleanup applies.

## Exact restoration

Keep bounded native conditional provenance from accepted REST/private-stream rows.
The native order fingerprint must match the Store row before destructive cancellation.
Do not reconstruct closePosition or priceProtect from the normalized reduce-only flag.
Require explicit reduceOnly/closePosition and a known workingType. A WS delta omitting
priceProtect can retain a previously observed flag only for the same unchanged native
order; otherwise refuse the amendment until a complete order read arrives. Conditional
orders with existing fills are refused before cancellation.

After a definitive replacement refusal, append one restore to the existing serial plan.
Reuse the original native type, side, trigger, workingType, priceProtect, positionSide,
quantity or closePosition, client ID, and conditional-limit price/time-in-force. This is
an original conditional order, never an immediate MARKET order. A confirmed restore
returns the existing typed Protection::Restored presentation. A refused restore returns
Protection::Unprotected with the symbol and the visible no-stop warning. No restoration
loop exists. An unknown replacement is not restored because it could already be live;
an unknown restore claims neither restored nor unprotected and remains visibly unknown.

The native snapshot is capped at 10,000 live orders and pruned by terminal events and
clean order censuses. Admission reserves capacity for the optional restore in advance.
Other venues and the engine API are unchanged.

## Official API contract

[Aster V3 New Order and Modify Order](https://asterdex.github.io/aster-api-website/futures-v3/account%26trades/)
documents workingType, priceProtect, closePosition and client IDs unique among open
orders. Its native modify endpoint supports LIMIT orders only; conditional stops and
TPs cannot use it. [Private order updates](https://asterdex.github.io/aster-api-website/futures-v3/user-data-streams/)
carry native workingType and closePosition; the documented example omits priceProtect.

## Native controls

`account.rs` covers stale/wrong-side admission, final-claim price movement and age,
claim unwind followed by a valid amendment, exact original restore, close-all/hedge
parameters, V3 signed TP success and restore, typed restore failure without retry,
unknown replacement without a second stop, incomplete native flags, and same-order
WS omission of priceProtect. `trading.rs` retains existing wire and capacity controls.
Three fixtures now seed a real native open-order read and fresh ticker instead of
only a normalized Store row; their original wire, outcome and capacity assertions
remain, except the old no-restore test additionally refuses the new one-shot restore
before asserting the same no-stop warning. The pre-cancel no-POST assertion is scoped
to order POSTs because real startup also creates a listen key.
