# Bybit take-profit trigger references

A TP add or amendment must send `tpTriggerBy` even when the native order had
no initial TP reference. Bybit can refuse an omitted value as UNKNOWN. Attached
virtual TP amendments, full-position TP changes and the existing missing-mark
partial fallback now send the real MarkPrice, LastPrice or IndexPrice reference,
defaulting to LastPrice when absent or unrecognized. Fresh TP placement and
partial recreation preserve IndexPrice too. Stop-loss defaults are unchanged.

An attached TP leg uses its own `tpTriggerBy`, rather than inheriting an unrelated
parent opening-trigger reference. Active native order levels with unrecognized
references emit a fixed warning without native strings, IDs or account data.
Unknown values remain unknown on read; the TP write boundary applies the default.

Controls ingest native linear, demo, HyroTrader and inverse order fixtures, then
assert the actual amendment bodies for attached, full and missing-mark paths.
They cover known, UNKNOWN, future and absent references; explicit TP add also
pins all three known references. Existing SL and partial-restore controls remain.
The shared fixture supplies matching fresh mark, last and index prices; this
keeps IndexPrice admission tied to the reference actually selected.

This does not change partial-stop cancel/recreate admission. Its physical-release
prevalidation is a separate PR. The production events did not retain a native
request body, so the omitted-field defects are established from code and fixtures
without claiming which amendment path produced those events.

[Bybit amend-order reference](https://bybit-exchange.github.io/docs/v5/order/amend-order)
requires a TP trigger reference when there was no initial value.
