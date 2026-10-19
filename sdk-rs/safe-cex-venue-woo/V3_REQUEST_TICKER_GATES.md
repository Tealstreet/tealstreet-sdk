# Historical V3 request/ticker scope (retired)

The inactive request preparer and account/write closure were retired earlier.
The remaining ticker composer, BBO/Trade DTOs and their exclusive
`v3_request_ticker` facade/oracle/control files are now retired too. No shipped
venue or consumer called them. See `TICKER_RETIREMENT_REVIEW.md` for the current
caller and registration proof. The earlier record/case counts are historical
and are not claimed as retained coverage.

The independent shared public mapper, numeric/declaration tests and constructed
legacy WOO ticker/order/socket paths remain. No V3 migration gate or runtime
registration is enabled, and no TypeScript source is changed.
