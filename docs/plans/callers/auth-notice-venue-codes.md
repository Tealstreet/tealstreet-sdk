# Auth notice venue codes

`Cx::startup_read_refused` now emits the existing typed error through
`notice_error`, retaining its kind, numeric venue code and bounded diagnostic
fields instead of sending only its message. Its one-shot, secondary-key,
already-loaded, startup failure and polling guards are unchanged. This covers
typed startup reads from Binance, BingX, AsterDEX, ApeX and WOO. It does not
manufacture a code for a caller that supplied none. The engine's existing
classification-aware IP-hint formatting now also applies to this notice, as
it already did to the typed startup failure.

Binance Options retains the actual -2015 before reporting the key refusal.
Bitget's terminal startup notice and Bybit's terminal key notice reuse their
already-typed startup failures. Bybit private WS auth notices retain supplied
integer `ret_code` or `retCode`, without using
them to decide auth success, changing clock repair, or inferring a code from
`ret_msg`. Missing, fractional, malformed or conflicting values remain absent.
Ordinary code-less private auth frames remain valid and code-less.

The producer audit/proposal is in SDK PR #3001. Its Bitget route-refusal
and route-exhaustion fixes are separate; WOO WS diagnostics remain owned by
held #2996. Local Hyperliquid mismatches,
Coinbase HTTP-only refusals, relay/token failures and local missing-credential
errors gain no invented venue code. HTTP status is not a vendor code.

No worker serializer changes, credentials, endpoints, auth admission, order
outcomes, retries, locks or caps change.
