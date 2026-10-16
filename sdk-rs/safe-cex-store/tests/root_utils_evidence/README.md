# Root utility slice evidence

Moved **16 rows pending->ported and 6 pending->divergent**, measured by tomllib
against integration `2bc83d78d4f821c4b116d082d02c7b5a2595ffc5` at exact target
`5962ef88f6f6564bad22ae6bcb9a7633d80fcbc7`. Nine assigned files change from
(ported, divergent, pending) **(8, 0, 87) -> (24, 6, 65)**. `movement.json`
records each file and independent expected 22-row movement; enumeration stops
at file nine (95 symbol rows). This is a census, not full behavioral proof.

The six divergences are four CHOSEN config initialization seams (host supplies
startup environment/sample and retains values) plus two FORCED telemetry name
surfaces. The latter witness actual source callback reading U+D800 before JSON;
Rust's required &str cannot retain it, per MANIFEST-SPEC §§14/19. Opaque event
arguments, context and failure values retain native identity. Trait plus optional
callback declarations are mapped as real split targets, not fake symbols.

`results.json` records exact commands, measured exits, SHA and oracle environment.
At this target: scoped all-target check, host clippy, wasm clippy, four native
root utility tests, control runner and extractor all exit **0**. The extractor
reports 7341 symbols, 0 updates, 0 added, 0 orphaned. All cargo processes ran
serially with jobs=2. No source changes remained after controls.

Three production Rust variants compile and exit **101** at named native
assertions: changed backoff constant, exchanged broker-field lookup, and erased
telemetry event failure. Restored Rust target exits **0**, 4/4. One actual-source
TS mutation replaces U+D800 with U+FFFD and exits **1** at the named callback
unit assertion; restored Node oracle exits **0**. Enumeration stops at control
four; no survivor in those four iterations. Native field/identity assertions
precede source-wire comparisons. This finite scope does not prove every input.

**Global Rust parity is FAILING, exit 1.** It emits the same 24 diagnostics as
an independent git-archived pre-port snapshot at
`f5e8057d338c930019f5eb81846ee8b9b58d797f`: AsterDEX/WOO rename correspondence
and three inventions in core/AsterDEX. `baseline-parity.json` and logs record
the baseline command and emitted diagnostics; final results record exact equality.
The validator source is unchanged across that baseline and target. The first
snapshot attempt lacked external infra/tradetunnel frames and failed to load;
`baseline-incomplete.*` preserves that separate reconstruction failure. Adding
those same-SHA source files enabled the real baseline check. Emitted-diagnostic
enumeration stops at 24. Equality establishes no new diagnostics, not a green gate.
No checker, exemption, allowlist, or inherited venue manifest was weakened.

Earlier measured failures are retained rather than transferred to this SHA:
- 021d1a0cb38fe77b2571b20a7b81acc79a600929: check/root tests 0; control runner 1
  because a compiling broker mutation (101) hit an earlier unnamed assertion.
  Config mutation was caught; telemetry/restored final gates were not reached.
- 78dda34609976cf1fa2468530ea81d5aab48ccdd: check/tests/controls 0; host clippy
  101 on a constant assertion. Subsequent gates were not run there.
- 1f76542dd31ae45bb2e9eed916cb16d2fd72d338: global parity 1 included two new
  telemetry trait-method inventions; those were mapped before this final run.

Actual inherited head: `74d0b79e1dddb094fd147ddf161cd4ed6b189a40`.
Direct parent of the first port commit: `f5e8057d338c930019f5eb81846ee8b9b58d797f`.
Inherited timestamp origin (including boxed ticker):
`738110c7040b9b9acd7081f26b8211620ee9c8fe`. Its final scoped lint and review are
bound separately to e571841d02 in the types/ticker_timestamp_evidence directory.
The inherited branch contains work outside this slice's review scope; do not
interpret this delivery as an overall-branch review or integration merge approval.

65 assigned rows remain pending, with empty Rust targets and implementation-gap
notes: accessToken 12; index/factory 8; metadata 8; account converter/factory 2;
REST diagnostics 26; WS diagnostics 9. Registry descriptors, credential policies
and injected host hooks do not substitute for those implementations.

Not run: full workspace builds/tests/lint, full crate suites, wasm execution,
binding-generator freshness, networking or automatic venue/host adoption gates.
This later document-only evidence commit does not change the measured Rust,
manifest or test target; no measured exit is attributed to its later SHA.
