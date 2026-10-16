# Historical witness findings

Measured base `1001271f8a73baee2b6b31a59db689ef40bc01fa`, parent `198f871b9b004f6384d910a8c3af5909bd706b48`. These exits cover the old witness only, not corrected `cafdd2d601`.

`controls.json`: planned8 controls, executed floor6, stopped index5 place-batch. First five were named runtime101/restored0. Sixth exited101 at compilation because the mutation made append unused; restoration0. That compilation failure is NOT runtime proof. The two listener controls were not executed here.

`survivor.json`: one control, stopped index0 forward-finding. At src/order_writes.rs::cancel_orders, forwarding the series dependency result wrongly passed (0); restoration passed (0). Cause: tests/order_writes.rs::Host::series customSeries branch returned undefined instead of the actual oracle dependency's opaque object. This is an observed witness finding, not a production mismatch. Corrected witness and same mutation runtime101→restore0 are recorded separately at cafdd2d601. Logs, exact commands, SHA, and patches retained.
