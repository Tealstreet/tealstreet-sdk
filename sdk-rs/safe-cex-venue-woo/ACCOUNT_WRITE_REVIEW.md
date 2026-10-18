# Inactive V3 account and order writes

Rows 6–7, 13–16, 47–48, 53, 55 and 57–58 are ACCEPT. They have no production
TypeScript caller and never become members or globals of the constructed WOO
exchange. The Rust-only ports and their audit facades are retired together.

The removed modules are `v3_account_writes`, `v3_position_mode`, `v3_private`,
`v3_private_orders`, `v3_regular_writes`, `v3_algo_writes` and the orphan
`v3_request`. Their 172 distinct exported names were checked across 3,311 tracked
Rust files. The only callers were within this group and its six exact facade
targets, plus the request declaration case in the shared ticker fixture.

The maintained source graph from `woo.exchange.ts`, `woo.api.ts`,
`woo.ws-public.ts` and `woo.ws-private.ts` loads no V3 module. Outside audit tests,
the V3 private declarations are imported only as types by the inactive write
modules. The actual source/native registration probe checks all 47 assigned V3
identities, eight false migration gates and two successfully constructed source
receivers. Its four native tests pass before and after removal.

All 165 affected source manifest identities remain as not-applicable, with this
caller evidence. Only the 36 orphan accepted contracts are removed; every
remaining contract and native observation stays byte-identical. The literal and
validator runners keep their exact remaining 12 and 13 owners. Public validators,
typed records, ticker/quote cases and live timestamp controls remain.

The CI WOO command replaces the four retired write-facade targets with the real
`binding_reads` and `binding_streams` targets. Signed legacy balance, leverage,
order placement/cancellation, history, sockets and cleanup are still exercised.
No production TypeScript behavior or V3 migration gate changes.

Validation uses the constructed source/native registration target, signed legacy
read/stream targets, remaining ticker fixture, strict all-target Clippy, format
check, the full parity index and the all-features cancellation inventory.
