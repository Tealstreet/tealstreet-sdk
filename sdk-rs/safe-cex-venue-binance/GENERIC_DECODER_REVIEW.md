# Generic decoder observations

The two accepted-set records retain the actual compiler-selected functions and
exact generic signatures. They execute concrete decoder profiles; neither is a
capability exemption or a claim about every type parameter.

| Owner | Fixed profile | Native observations |
| --- | --- | --- |
| `class_decode::decode` | `T = (f64, bool, String)` | Three tuple results, binary64 bits including negative zero, boolean and text members; eleven malformed roots, lengths or members |
| `deserialize_present` | Owned `serde_json::Value`, `T = Option<f64>` | Present outer option, explicit inner null, binary64 bits including negative zero; four rejected roots |

Both adapters call the production function directly. They inspect returned fields
before constructing diagnostic JSON and never serialize the decoded owner.
`class_decode` receives native `Js` through production `from_json`; this finite
profile does not cover arbitrary prototypes, accessors or reference graphs.
The nullable optional profile is deliberately separate from the generated wire
schemas' concrete field types.

The independent execution witnesses remain the actual constructed native
`UpdateOrder` path and the qualified public Futures position-risk vector decoder.
The former constructs production dependencies, clients, signing and Store, then
observes signed cancellation followed by replacement placement. The latter checks
present/absent optional wire fields, row order and binary64 bits. Terminal HTTP is
the only substituted dependency in the constructed order witness.

Run `scripts/generic-decoder-controls.py` to replace the native DTO input with
undefined and independently replace the wire helper's outer `Some` with `None`.
Each fault must compile and fail its named production witness; each exact source
restoration must pass. The script rejects compiler failures as proof.

Run `scripts/generic-decoder-contracts.mjs --expect-unclassified` before adding the
records and run the same script without that flag afterwards. The maintained
accepted-set driver performs full dependency discovery and checks all source
pins, selecting only these two owners for receipts. Both named missing-contract
failures and both positive runtime results must be inspected; a successful
`--measure` process exit alone is insufficient.
