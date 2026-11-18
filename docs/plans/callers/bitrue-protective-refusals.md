# Bitrue protective replacement refusals

## Contract

Guarded stop-loss and take-profit replacements classify the explicitly listed
placement validation and business refusals from the [official Bitrue Futures
error list](https://www.bitrue.com/api_docs_includes_file/futures/index.html#error-code-list)
as definite refusals. These cover invalid order parameters, precision, quantity,
price, contract state and balance. This is an explicit allowlist, not a numeric
range or an import of Spot error codes. The original `BITRUE` table remains
unchanged for ordinary orders, cancellation, reads and settings.

The replacement leg has managed `Place` intent. Its response-local classified
fact supplies both the managed write verdict and the reported typed error. A
coded refusal is accepted only without order data. Rate-limit statuses (418 and
429), the native request-frequency code (-1003), 408, 5xx, unlisted codes,
malformed replies and error envelopes containing order data remain Unknown.
They never trigger a put-back or resend. A rate status cannot become a definite
refusal through a contradictory error body.

After target-terminal cancellation proof, a definite refusal may put the
original protection back once. That put-back must be a single order with the
original venue quantity, order type and trigger. If current precision or limits
would round or split it, or the original has an attached/group shape, the result
is typed Unprotected with a request to re-place protection. No guessed restore
is sent. A refusal of the one put-back is Unprotected; an ambiguous answer stays
Unknown. Existing final-claim trigger receipts, order/fill/position checks and
cancel-proof polling remain in force.

A directly amended attached stop-loss or take-profit is refused before any
cancel or create, with a message to re-place through its parent. Its existing
protection stays untouched rather than being recreated as a standalone order.

## Controls

- Real-engine documented-code cases require one exact original put-back and no
  second put-back after that is refused.
- Rate, timeout, server, unlisted and contradictory-data responses require an
  Unknown outcome with no put-back.
- Both attached leg kinds require a NotSent outcome, no cancel/create and an
  unchanged open order.
- Original quantity/trigger rounding and quantity splitting require Unprotected
  rather than a guessed put-back.
- `ordinary-amend-master.json` was captured on master `06a9b1326ac`, before this
  change, using synthetic credentials. Eight ordinary replacement cases pin
  exact cancel/create URLs, bodies and ordered signed headers, full typed errors
  and absence of further order posts. There is no fixture-update mode.

The new documented-code, attached-leg and typed-Unknown controls fail on the
unchanged base. A separate original-quantity control reproduces its rounded
put-back before the exactness guard is applied. KuCoin adoption originals and
CI invocation are outside this change.
