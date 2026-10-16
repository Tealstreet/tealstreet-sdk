# OKX accepted-set review

The first reviewed batch covers 24 exact compiler owners. Its production native
probes execute the independently authored contract witnesses and report zero
failed owners. This is a scoped batch, not a complete OKX or dependency-closure
gate. Ordinary compiler discovery still has 145 owners and four aliases before
the outstanding private string-enum repair.

| Reviewed group | Owners | Native evidence |
| --- | ---: | --- |
| `OkxRegion`, `OkxInstrumentType`, `TickerMarkInput` | 3 | Exact native variants; actual record fields, optional presence and numeric bits; ordinary and qualified shape checks. |
| Numeric coercion, finite parsing/summing and optional history numbers | 7 | Independent binary64 bit strings, malformed-input/fallback behavior and native optional/error results. |
| API endpoint, signature rejection and error-message helpers | 3 | Actual native predicates and selected/formatted errors, including fallback and precedence cases. |
| Income nullish/truthy helpers | 3 | Actual string/presence/boolean results for distinct false, nullish and coercing inputs. |
| Socket sequence and REST error-message helpers | 2 | Actual native string/error results, including nullish and fallback behavior. |
| Global/EU market filtering, suffix, trigger, instrument and symbol helpers | 6 | Native policy variants, predicates and exact transformed strings on distinct branches. |

Each function contract pins its complete compiler signature and generic
parameters and calls that production function. Expected values were authored
from the input/source contract before executing Rust; they are not harvested
from the implementation. Fixture JSON encodes the arguments of those actual
typed signatures; the adapters add no production JSON entrypoint. Observations
inspect native results directly rather than using a serialization round trip.

The private `RegionWire` and `InstrumentWire` decoders independently admit tagged
objects such as `{"okx": null}` and `{"SWAP": null}`. Their outer public enums
already have string admission guards. They remain owed individual repairs under
the [string-only enum recipe](MAP-ONLY-INGRESS.md#string-only-literal-enums);
neither private visibility nor the passing public enum authorizes that widening.
No contract for either private helper, exemption or standing-debt permission is
added by this batch. The remaining owners and all four aliases also remain owed.

The ordinary package gate is
`accepted-set.mjs --package=safe-cex-venue-okx --inventory --jobs=1`; it continues
to refuse unreviewed keys. This batch's focused native evidence used fresh
pinned Rust 1.99 rustdoc discovery, authenticated selected Cargo artifacts and
the production `runProbes` with `configuration.jobs = 1` for the 24 exact owners.
Do not substitute that selected batch for complete discovery or a zero-missing
claim. Accepted probes must run serially.
