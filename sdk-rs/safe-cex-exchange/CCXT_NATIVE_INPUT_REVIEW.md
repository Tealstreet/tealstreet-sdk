# CCXT native input owners

This cohort reviews the nine pending `js_runtime::ccxt` owners. The shared
installed imports are the production binding; the Hyperliquid Realm migration
keeps these owners and may add another global delegating to the same helper.
No serialized admission rule, runtime implementation or source exception is
changed by this classification.

| Owner | Role and real native probe |
| --- | --- |
| `decimal_to_precision` | Conversion: installed `decimalToPrecision`, including rounding/truncation, tick-size recursion, negative precision, malformed strings and original coercion errors. |
| `exact` | Non-decoder: strict native numeric-tag comparison, without coercing an object mode or accepting the string `"1"` as ROUND. Installed strict-mode probes exercise both outcomes. |
| `install` | Non-decoder: constructs constants and callable descriptors on the supplied native globals. Every installed import test executes it through `install_library`; the owner-route probe checks all nine constants and all five callables. |
| `integer` | Conversion: parseInt string-hint coercion, hexadecimal prefix, trailing text, undefined and a lone surrogate. Installed parsePrecision and scientific notation routes consume it. |
| `number_to_string` | Conversion: scientific notation expansion and direct non-number `toString` dispatch; installed probes retain arbitrary returned referents, thrown identity and UTF-16. |
| `numeric_string` | Conversion: the negative-precision decimal route's actual `toString()` call. Direct native receipts preserve raw units and the null refusal. |
| `parse_precision` | Conversion: source loop spelling, zero, trailing integer text, undefined and a lone surrogate. No invented numeric-only admission. |
| `precision_from_string` | Conversion: tick-size recursion over the canonical number string; direct receipts cover exponent, trailing zeroes, integer and lone-unit spellings. |
| `safe_string` | Conversion: property/key reads and string-hint conversion, retaining default referents, live coercion hooks, repeated reads and original errors. |

Each conversion has an exact compiler API, executed input/output receipts and
native observations constructed directly as `Js` values, before JSON output.
Numeric fixture inputs use exact IEEE-754 bit strings. Expected observations
read string code units, number bits, type and error units;
they never serialize the decoded input. `js_ccxt_imports_source.cjs` executes the
unchanged maintained `ccxt_exchange.ts`, `number.ts` and `type.ts`. The private
`integer` and `numeric_string` helper expectations use their source operations,
`parseInt(x)` and `x.toString()`, rather than a second formatter implementation.
The contract expectation check prevents those independently computed outcomes
from drifting away from the registered receipts.

The existing installed test corpus has 123 scalar cases plus a live getter,
coercion, return/throw identity comparison. The added owner routes reach every
helper through installed functions. These bounded matching results justify the
nine classifications; they are not a claim that arbitrary JavaScript programs
or all possible precision sizes have been proved equivalent.
