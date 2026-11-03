# Coinbase cash-root replay baseline

The #2970 spot balance correction enriched the existing private fixtures with
Coinbase's native FIAT/WALLET account types. Rebind the eight private tape SHA
identities to those exact bytes; every historical diagnostic key and rationale
stays unchanged. Public tape declarations are unchanged.

The four spot-profile variants additionally name the exact intended USD cash
root: total 1000, free 900 and used 100, versus legacy TS zero. Only these three
fields at the final snapshot and checkpoints 0 through 18 are accepted as
`rust-better`. BTC and USD currency differences remain their existing known
diagnostics; no request, transport, signature or workload failure is accepted.

SDK, NX and server callers need no change. CI must replay the generated private
tapes with the same declared SHA; new output values or diagnostic paths still
fail verification. A future fixture change must requalify these exact bindings.
This baseline repair changes no production behavior or WASM budget.
