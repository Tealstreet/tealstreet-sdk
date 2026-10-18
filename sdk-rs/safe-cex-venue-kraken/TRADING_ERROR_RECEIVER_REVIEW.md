# Kraken trading error receivers

The old concrete ErrorHost rejected every non-string before looking up its case
member. Live `KrakenExchange.placeOrder` and `setLeverage` accept callable
`toLowerCase`/`toUpperCase` members, with one getter evaluation, the original
receiver and original native result. Getter exceptions must propagate too.

The direct cutover is one shared native property lookup and call, with the
existing contextual TypeError only when the resolved member is not callable.
There is no model widening, local case conversion or replacement fallback.

`tests/support/accepted_trading_error_receivers.rs` invokes the actual Scope of
a constructed KrakenExchange, reusing only its terminal runtime/transport.
Lower/upper tests compile and fail on the original String-only guards, then pass
with native UTF16 casing, one getter/call, original receiver/result, signed zero,
primitive refusal and getter-thrown Error identity. The third test calls the real
BaseExchange console and error listener through `emit_and_log`; it checks native
Error identity, supplied context and the context-absent text branch.

`scripts/trading-error-receiver-source.cjs` extracts and executes those two actual
live TypeScript class methods. It loads the real enum/endpoint/severity modules,
uses an error-producing Store getter and terminal HTTP only, and asserts the
native message/symbol member calls and forwarded result identities. Run from the
repository root:

```sh
SAFE_CEX_TYPESCRIPT_PATH="$PWD/node_modules/typescript" node sdk-rs/safe-cex-venue-kraken/scripts/trading-error-receiver-source.cjs
```

The three independent production controls restore the String-only guards and
corrupt the forwarded context. Each must compile, fail its named native assertion
and pass after exact byte restoration. Compiler errors and zero-test runs do not
count. The receiver policy pins the complete current compiler ErrorHost impl;
the two unchanged nested qs helper file pins are reviewed/refreshed as well.

Canonical receiver refusals decrease from seventeen to fourteen. Eight remaining
methods belong to codex-0xekzz (SpotCancel3, Account2, Class3); this lane retains
Trading2, Api3 and Tick1. No full-crate or following-alias pass is claimed yet.
