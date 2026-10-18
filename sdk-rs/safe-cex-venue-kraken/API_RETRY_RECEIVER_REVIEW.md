# Native API retry receivers

The original constructed-Scope corpus produced three named failures: a cancelled
GET was approved for retry; a missing-config signature error evaluated a throwing
response getter; shared Array.join changed a lone surrogate to U+FFFD. The shared
join fix is isolated in #1037. This cohort directly fixes only the first two
Kraken receiver bodies and observes the unchanged join delegation.

`retry_condition` follows live `utils/retryReadsOnly.ts` and the installed
axios-retry/is-retry-allowed functions. It preserves the separate live config
reads, native optional method dispatch, read-method short circuit, and distinction
between no posted-read filter and an empty posted-read filter. Writes stop before
network-error property evaluation. Response truthiness precedes the first code
read; the excluded-code check and deny-set lookup perform their own code reads.
Code comparisons retain native cells and do not invoke String coercion.

The existing transport deny list is reused as public immutable data. Its values
and every provider function body are unchanged; the six enclosing-file helper
pins are refreshed after verifying that sole visibility/documentation delta.
No response/body API or retry budget changes.

`spot_retry_condition` retains the actual short-circuit signature helper order:
missing config skips response access, approval evaluates response twice and stamps
the original config, and a repeated refusal is denied after one guard response
read. Posted reads use the network predicate without a signature stamp.

The two factory methods accept source-supplied static posted-path configuration
and return native callable capabilities, so their receiver classifications name
that specific non-decoder role. Their returned behavior has independent executed
coverage; this classification does not excuse callback semantics. `join` has a
separate delegated member-call classification and keeps its exact existing body.

`tests/support/accepted_api_receivers.rs` reaches the actual Scope of constructed
Futures/Spot exchanges. It observes exclusions, falsy responses/codes, non-string
and lone-UTF16 code values, exact getter ordering, skipped URL/error getters,
custom lower-case dispatch, original signature-config identity/stamp, one-retry
behavior, native join result identity and code units, getter/call counts and
original thrown errors. No production host, retry predicate or emitter is mocked.

`api-receiver-source.cjs` executes the live utility module with the installed
axios-retry package and AST-selects the exact Spot retryCondition/rejection arrow,
using the live signature helper and endpoint constants. It checks the same
short-circuit and stamp profiles. `api-receiver-controls.py` compiles one semantic
fault per concrete receiver: cancelled-code spelling, signature stamp and join
separator. Each must fail its named native witness, then pass on exact restoration.

The isolated canonical receiver inventory reduces eleven refusals to exactly
eight, clearing these three reviewed rows. After normally inheriting the separate
SpotCancel/Account/Class cohort (#1040), all 25 reviewed Kraken receiver guards
clear. The full inventory then stops honestly on the next generic alias:
`rest_account::Handle<H> = Rc<<H as ReadHost>::Value>` requires explicit finite
compiler coverage. This PR does not classify that alias or relax a shared
inventory guard or array/Store admission rule. The public denial-data declaration
is mapped to its existing source network-read policy with exact provenance.
