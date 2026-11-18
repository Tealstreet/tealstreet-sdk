# Lighter maker-only crossing message

The SDK's native `canceled-post-only` stream rejection now displays:
“Post-only order would execute immediately as taker; not placed.”
The event keeps `order.rejected` and its error severity, carries the typed
`rejected` kind, and retains the original native reason in `venueMessage`.
The order mapper still uses `RejectReason::PostOnly`. No caller should parse
the display string to classify it. SDK, hosted and browser callers receive the
message through the existing event path; no NX text mapping is required.

Freshness, duplicate suppression and silent user cancellations retain their
existing rules. No retry, order submission or cancellation behavior changes.
The maker-only API-key permission explanation remains separate.

Regression: a native `account_all_orders` rejection (status only, or either
cancel-reason field spelling) emits exactly one plain classified error, retains
other live orders, and emits no HTTP. The parent fails at the message assertion
with `canceled-post-only`. Existing account and transaction controls remain.
