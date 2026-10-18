# Native Kraken diagnostics

Two actual constructed-receiver tests fail on the previous production bodies:

- `PollHost::console_error` flattened the prefix and native error into one
  string. Live `KrakenExchange.listenOHLCV` calls
  `console.error('[Kraken] Error polling OHLCV:', err)` with two arguments.
- `ReadHost::trace` flattened the caught native value before calling the host.
  Both live Kraken exchanges call `console.trace(err)` with the original value.

The direct cutover uses the shared runtime's `console_values`. No value is
stringified before the terminal host chooses its representation. The existing
terminal fixture now records native arguments before applying its unchanged
`console_text` presentation, so existing textual observations remain available.

`tests/support/accepted_diagnostic_receivers.rs` gets the Scope from the actual
KrakenExchange constructor. It observes severity, exact argument count/order,
original Error reference, native subtype, lone UTF16 metadata, infinity and
negative-zero bits. No substitute PollHost/ReadHost implementation is supplied.

`scripts/diagnostic-receiver-controls.py` restores each original flattening body
independently. Both compile and fail the corresponding named native assertion;
exact byte restoration passes. The script rejects compiler errors and zero-test
runs and restores production bytes in `finally`.

The reviewed PollHost console method is a delegation to the shared runtime
sink, not an error decoder. It clears one of the eighteen remaining receiver
refusals. The other seventeen remain owned by this lane; the later alias census
is still blocked and is not claimed green. The existing seven ReadHost impl
pins and two enclosing qs-helper file pins are refreshed after reviewing this
trace-only change; their previously reviewed bodies and roles are unchanged.
