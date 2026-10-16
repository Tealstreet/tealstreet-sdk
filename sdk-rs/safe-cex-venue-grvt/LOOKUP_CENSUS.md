# GRVT property-access census

Baseline: integration `9222ed2454`. Reproduce the raw inventory with
`SAFE_CEX_TYPESCRIPT_PATH=<absolute isolated compiler> node sdk-rs/safe-cex-venue-grvt/tests/lookup-census.cjs`.
The TypeScript AST contains **95 element-access expressions**, including writes
and array indices; this is not a count of defects.

| Group | Expressions | Classification |
| --- | ---: | --- |
| Array indices | 33 | Numeric index or fixed numeric literal, not prototype-name input. |
| Header accesses | 15 | Fixed source header names, not wire-controlled keys. |
| Public socket handlers/topics | 18 | Immune: every key is prefixed with `kline.` or `orderbook.` in source. Includes reads, writes and decrement expressions. |
| WebSocket URL selection | 1 | Closed source: `testnet ? 'TESTNET' : 'PRODUCTION'`, not wire-controlled. |
| Local mapping tables | 3 | Two caller-provided interval strings; one signing TIF mapping whose caller constructs a closed TIF value. |
| Store lookup tables | 21 | Five existing uppercased fallback reads are immune to stock Object.prototype names. Sixteen other reads need source-path witnesses. |
| Currency accumulator | 4 | Three reads and one write keyed by wire currency. Needs a whole-operation witness; lookup and prototype mutation are distinct operations. |

The sixteen non-uppercased Store reads comprise seven direct wire-instrument
lookups (exchange positions/order mapping, four private socket paths and public
ticker), four keys derived from stored positions/markets, and five caller-symbol
lookups. Derived keys remain wire-influenced: a store constructor cannot be
replaced by Object.fromEntries in witnesses without masking prototype writes.

Thus **ten direct wire-keyed reads** require investigation: seven Store reads
plus the three currency accumulator reads. The one wire-keyed currency write is
tracked separately. Two interval mappings, four stored-key reads and five
caller-symbol reads add eleven externally influenced reads. None is declared
ported by this census. Source-path witnesses and output-boundary classification
belong to the later mapper/socket slices.

No immunity is being added. Open string outputs must carry member-specific
`ProtoMember::js_string`; closed typed outputs must cite
`sdk-rs/parity/MANIFEST-SPEC.md#closed-typed-output-boundary` and witness whole-record
refusal separately from function omission and the surviving `__proto__` object.
