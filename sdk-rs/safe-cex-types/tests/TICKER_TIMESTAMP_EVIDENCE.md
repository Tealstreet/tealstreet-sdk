# Ticker runtime timestamp

The actual Paradex `fetchTickers` method returns `timestamp` from
`virtualClock.getCurrentTimeMs()`, in milliseconds. The shared TypeScript
`Ticker` declaration does not declare this runtime extra. Rust retains it as
`Option<f64>`; generated TypeScript keeps the declaration projection through
`ts(skip)`. The exact-field checker remains unchanged at 438 fields.

Before widening the type, the TypeScript AST census inspected 303 venue source
files and 134 explicit timestamp properties. The direct shared-Ticker emitter is
Paradex. The external-public-data handlers forward received ticker records
without constructing a timestamp. Backpack spreads response records into `info`,
and MEXC normalizes its spread input before applying an explicit ticker patch.
Lighter SDK and WOO V3 timestamps belong to their separate wire types. This is
an inventory of source emitters and forwarding paths, not a claim about arbitrary
payloads supplied by remote gateway servers.

The later actual-method oracle executes the extracted method once per corpus.
Four input markets produce three tickers and three clock calls. Before JSON
serialization it captures three timestamp bit patterns, including negative zero,
with architecture, Node version and source fingerprint. All three are finite;
none is NaN. The original shared numeric and model feeder artifacts are unchanged.
This new oracle feeds the num crate's dedicated timestamp target and makes no
retroactive independence claim.

The target reads native timestamp and quoteVolume fields directly: six fixed
field comparisons over three independently expected clock events. Canonical
raw, owned and borrowed ingress separately checks six numeric spellings and
eighteen bit comparisons, including both overflow signs and lexical negative
zero. Absence is None; present null is refused by the numeric-field boundary.
This does not claim generic gateway payloads refuse null in TypeScript, or that
plain serde accepts JavaScript overflow input.

The Store target pins insertion, three numeric patches, unchanged quoteVolume
and nine executed patch comparisons across id/symbol aliases. A full replacement
with an absent timestamp clears both aliases; insertion is asserted in both.
Worker fast-path patching and packed deltas retain the source's restricted
field inventories; this change does not refresh timestamp through those paths.

The original agent reported four independent production mutations compiling and
exiting 101 at named assertions. That report did not record its measured SHA;
these historical results must not be attributed to a later commit:

- Timestamp/quoteVolume serde names swapped: `TICKER-TIMESTAMP native clock identity`.
- Timestamp signed zero erased: `TICKER-TIMESTAMP native clock identity`.
- Timestamp patch dropped: `TICKER-TIMESTAMP Store numeric patch identity`.
- Timestamp alias copy dropped: `TICKER-TIMESTAMP Store numeric patch identity`.

The original agent reported each target restoring to 1/1. These reported controls
exercise the timestamp boundary and alias copy,
not complete Paradex exchange execution, networking, clock implementation,
all ticker fields, or all numeric output graphs.
