# Assigned OKX FIX-audit rows

The exact five rows from `assignments/codex-0efu4x.json` are recorded in
`tests/divergence-rulings.json`: two FIX and three bounded ACCEPT decisions.
No source mapping, identity/alias guard or existing corpus count is removed.

Normal and algorithmic placement previously rejected a successful numeric ID
after the order was already live. Both public SDK probes compile and fail on
the previous production binding with `OKX order id must be a string`. The one
existing placement algorithm now applies shared JavaScript String to each
truthy ID, following the coordinator's extension of shared-type decision 4.
Missing/falsy IDs remain visible errors. Serial requests, order and later
source-error precedence are preserved. Four global/EU live/demo constructors
run each public SDK probe. Installed batch-member probes additionally cover
numbers, booleans, arrays, objects and strings, two serial requests, and later
missing/zero/null rows.

The maintained 118-case REST oracle still runs the same 32 lookup, 43 normal
and 43 algo inputs, including all ten later-request/error cases. Eight chosen
ID expectations now retain their original source result separately and compare
the native String conversion. The actual source functions and all source
mutation controls are unchanged. Malformed data and missing IDs still reject.

The private parseFinite/sumFinite residual concerns executable coercion hooks,
sparse or cyclic objects, which JSON socket input cannot construct. The real
installed members process HOW-decoded wire inputs over all four constructors,
pinning finite values, signed-zero bits, overflow/underflow, arrays/objects,
ordered sum and thrown coercion. A numeric negative zero becomes positive zero
through source String(-0); a string "-0" preserves its negative-zero parse.
The existing 35-case actual-source oracle and actual private position-frame
Store tests remain unchanged. No callable-object parity is claimed.

The class row's explicit host composition and own-member/prototype choice are
accepted under the coordinator's category rulings. Real construction preserves
region, forced demo testnet and the original socket parents. The existing
constructed-family test separately asserts distinct socket owners sharing the
original native Store. Neither test substitutes a constructor or socket body.

The wrapper census classifies OKX as eager JS invocation. Its call/invoke/unit
boundary already invokes the actual member before boxing a waiter and awaits
that same Promise. No additional start_eager or per-method spawn is required.

`tests/divergence_controls.py` independently breaks normal ID conversion, algo
ID conversion, installed parse, installed sum and constructor region selection.
Every fault must compile, fail its named real-binding assertion, restore the
exact source bytes and pass the same test. Run from sdk-rs with the isolated
TypeScript path set. Logs go to `target/okx-divergence-controls/`.
