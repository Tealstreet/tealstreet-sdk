# Native index-ticker lookup

The production binding previously decoded every complete `Market` before
selecting index-ticker rows. The maintained TypeScript methods read only
`info?.uly`, or the cold `id.replace` fallback, through the live Store array.
The new binding follows those reads and calls the array's actual `filter`.
It preserves original row references, dynamic helper lookup, strict comparison,
sparse-array behavior, source coercion and thrown identity. It adds no model cache.

Compiled baseline regressions fail on unrelated getter reads, partial rows,
custom filter dispatch and retained UTF-16. A second compiled RED catches the
predicate's observable name/arity. The fixed real-binding tests and original
TypeScript AST oracle pass, including callback length descriptor assertions.
The enclosing-file policy pin changes; the nested price-load reader is unchanged.

## Matched RELEASE proof

The full original OKX tape contains 285,713 frames and 34,481 index-ticker
frames over 509 markets. The maintainer approved **one** control/candidate pair
for this slice, with two native and two TS replays in each report. This is not
the three-pair gate reserved for the larger socket state-copy repair.

Both artifacts use the same pinned compiler, lockfile and explicit optimized
`--release` build, with `CARGO_INCREMENTAL=0` and no profile overrides. The frozen
control is `a29d540656`; the isolated lookup candidate is `ef932d7ca7`. Later
master integration is validated separately and does not change this experiment.

| Feed-to-drain measurement | Control median (range), seconds | Lookup median (range), seconds |
| --- | --- | --- |
| Native process CPU | 944.900 (907.635–982.165) | 726.365 (725.192–727.539) |
| Native wall | 2138.729 (2076.050–2201.409) | 1571.171 (1568.307–1574.036) |
| TS process CPU | 64.501 (64.005–64.998) | 54.784 (54.319–55.249) |
| TS wall | 356.011 (352.514–359.509) | 284.551 (281.460–287.642) |

The measured native CPU median drops **23.13% (1.301×)**. CPU is primary;
compiler and replay overlap on the shared machine is recorded, and every sample
is retained. TS CPU also drops between reports. This single pair does not
isolate a causal speedup, and wall time is not an isolated performance claim.
The candidate remains **13.259× TS CPU**: the **≤2× target is still open**.

All four native runs verify the entire input with no drops, physical and
application drain receipts, and nonprovisional results. Their complete retained
outputs are observably identical: 919 emissions, comprising 239 Store updates,
593 books and 87 candles. The comparison checks ordered own keys, descriptors,
prototypes, `Object.is` including signed zero, and both directions of alias
identity. Eight positive/negative checker controls pass. Each output file's
SHA verifies integrity. Serialized V8 bytes may differ for the same positive
zero (Smi versus HeapNumber), so byte inequality alone is not an output change.

Both frozen artifacts retain the same known TS metadata differences: three
health own-undefined fields, fixed separately by merged #1167, and the business
candle `hadHandlers` bit, owned by #1218. These differences remain visible in
the raw reports; no field is normalized or omitted by this proof. Books,
candles and ticker Store values compare exactly against TS. This is output
identity across the isolated repair, not a claim of complete TS parity.

`index-lookup-release-evidence.json` records all samples, exact source/addon/tape
and harness hashes, commands, output integrity hashes, receipt details, graph
checks and known shared-load intervals. Raw reports and V8 fixtures remain at
the paths recorded there. An earlier run interrupted by Sam's stop order is
preserved separately and never counted as completed evidence.

Reproduce with the unchanged `streams.mjs` runner and frozen artifacts, then:

```sh
node sdk-rs/safe-cex-venue-okx/tests/index-lookup-throughput.mjs \
  CONTROL_REPORT.json LOOKUP_REPORT.json
```

The checker requires the approved sample count, full native drain, complete
observable output identity and a lower native CPU median. It reports the wider
two-times-TS target independently.
