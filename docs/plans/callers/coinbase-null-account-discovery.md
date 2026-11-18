# Coinbase discovery without a US futures account

A successful CFM balance-summary probe may return JSON `null` or
`{"balance_summary":null}` when the user has no CFM account. During Coinbase
account discovery, both forms now select the existing spot fallback, just as an
unavailable CFM account already did. A valid summary still selects CFM. Private
market, ticker, balance, order and position startup can finish without retrying
the absent futures account.

Null is not a zero futures balance. After CFM has already been selected, a null
balance response leaves the previous balance intact and reports an unknown
balance. Missing fields and malformed non-null responses remain errors; their
diagnostic names path `/api/v3/brokerage/cfm/balance_summary` and field
`balance_summary`, without response values. Required financial amounts remain
strictly validated.

SDK, NX and server callers need no migration. Coinbase Spot keeps its existing
direct spot discovery. This change adds no trading, hosted-signing or deletion
prerequisite. No credentialed requests are needed to reproduce it.

Endpoint reference:
https://docs.cdp.coinbase.com/api-reference/advanced-trade-api/rest-api/futures/get-futures-balance-summary
