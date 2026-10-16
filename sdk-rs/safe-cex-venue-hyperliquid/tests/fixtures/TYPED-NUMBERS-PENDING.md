# Typed numeric ingress evidence

The actual Node corpus was committed first at f9e71460f5: 53 source numeric
roots and 2,380 raw-input/pre-serialization bit cases. Expected bits were never
adjusted to Rust. The rejected Value-first candidate traded overflow repair for
signed-zero loss; it was not adopted.

`../typed_numbers.rs` is now an active Cargo test. Public record and untagged
union Deserialize captures JsNumberInput and decodes through existing private
wire companions and strict map visitors. This is deep adoption: bare raw string,
raw slice, owned Value, borrowed Value, js_from_str, js_from_value and
js_from_value_ref all compare f64 bits before serialization, 16,660 comparisons.
Raw overflow becomes Infinity; signed zero remains negative zero on every route.
The dedicated existing-input audit also checks native and lexical Value forms.

The 49 numeric roots with open strings retain their separately witnessed CHOSEN
UTF-8 boundary. Their 147 witnesses preserve actual-source lone-surrogate
acceptance and scalar/astral/BOM positives. AssetCtx and SpotAssetCtx have six
additional witnesses after their independent optional/null repair. Numeric
success alone never establishes full type parity.

Production capture bypass controls for TriggerOrderType and OidOrCloid compiled
and failed at runtime on named 1e400 cases. Optional-null guard removal compiled
and failed on OrderType.limit; required-nullable guard removal compiled and failed
on missing SpotAssetCtx.midPx. Widening the source checker annotation to any fails
the named rejection assertion, also required by a permanent child-process test.
These controls do not assert a whole-venue completion or branch-CI result.
