# Concrete snapshot native notification proof

Source revision: `b654fe814138b5069b22e6246f879f9aad537ac5`.
Merged integration revision: `b575df520a974c61587111341dc290fbb7722f67`, incorporating
origin `53ee502232b005173d8a6f3f0d7cf94bad7f072c`.

`source-receipts.json` records the three scoped native test passes and fourteen
Node audit passes at the source revision, exact commands and zero scoped source
diff through the integration merge. `source-hashes.json` identifies the eight
maintained-source/oracle inputs in this finite iteration.

`controls.json` and the six paired logs record three compiled production mutants,
each exit 101 at its independently named native assertion followed by restoration
exit 0. They discard the native origin, swallow the concrete class publication
error, and set the class loaded flag incorrectly. No surviving control is omitted.
Native cause/root/state assertions run before any source wire projection.

`census.json` parses all sixteen Coinbase manifests with tomllib, stops at
`coinbasespot.exchange.ts.toml`, and compares the exact 426-row identity set with
both the working parent and actual integration parent. Exactly the snapshot row
moves divergent/FINDING to ported, with three real existing native targets;
zero pending rows move. Counts are 393 ported / 33 divergent / 0 pending after
this change, versus 392 / 34 / 0 at origin 53ee. Older origin 1810 measured
393 / 33 / 0 before the snapshot withdrawal reached it; no row gain is claimed
against that older revision.

The new native real-Store scenario exercises the actual class dispatcher and
snapshot entry, including committed values inside a failing subscriber, retained
original cause/subscriber identity, assembly reset, suppressed later fanout and
events, retained class roots, and recovery. The source oracle floor is the six
named class scenarios ending at notification recovery. It does not establish
live replacement of the imported Store or a full CoreStore alias-preserving class
binder. Sixteen total Coinbase FINDING rows, including fourteen notification
roots, remain open.

Shipping gates run after the tracked evidence commit. Their exact handoff SHA,
commands, exits and logs are saved locally under
`.aimux/coinbase-notification-handoff/<handoff-sha>/gates.json`.
The tracked source receipts do not stand in for those final gate measurements.
Wasm and the central static sweep are delegated to integration by the user;
remote parity-job completion remains unconfirmed because jobs are cancelled.
