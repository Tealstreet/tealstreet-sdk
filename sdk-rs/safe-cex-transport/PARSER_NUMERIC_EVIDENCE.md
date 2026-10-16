# Remote-signing JSON replay normalization

The actual `utils/tryParseJson.ts` oracle was captured first at 3ff540877d,
before the Rust repair. Its 62 cases observe numeric bits, String and truthiness
before serialization across objects, arrays and nested objects. Inputs cover
finite alternate spellings, integers beyond 2^53, exponent boundaries,
subnormals, negative underflow, signed zero and positive/negative overflow.
Invalid strings, JSON primitives and existing non-string inputs stay unchanged.
The TypeScript module itself executes; no handwritten replacement is compared.

Rust formerly retained finite source lexemes such as 1.50 rather than 1.5,
and 9007199254740993 rather than the binary64-rounded integer. Successfully
parsed object/array strings now reuse HOW's numeric and JS own-key projection.
Overflow and negative-zero lexemes remain available to canonical coercions.
No Value-first typed-f64 decoder is introduced and non-string input is untouched.

Removing the production normalization call (and its now-unneeded mutable
binding) COMPILES, then exits 101 at `finite signer payload must collapse its
numeric lexeme to binary64`. Restoring it passes all three new tests with the
isolated compiler. This assertion pins the representation handed to signing
consumers, while bit/String/truthiness observations independently pin numerical
meaning before JSON null can erase overflow. Ordinary existing Value payloads
remain unchanged; their original ingress belongs to their owner.

The native Cargo matrix discovers this integration test and supplies its
existing frozen isolated TypeScript install. No new npm selector is needed.
The fixture generator also passed in a disposable tracked-source mirror outside
the checkout with no ancestor node_modules, using that isolated compiler.
Pointing its real compiler import at a missing package exits MODULE_NOT_FOUND;
no missing-tool skip or fallback was added. The mirror was removed afterward.

Scoped all-target transport suite: 147 pass, three inherited explicit external
oracle tests ignored; no new test ignored. Host and wasm all-target strict
clippy each exit 0. 230 CI wiring controls pass, and parity passes at 7334 rows /
2514 public Rust items. This is finite normalization evidence, not a full JSON
parser accepted-set claim; shared UTF-8 and parser-depth boundaries remain.
GitHub access remains unavailable, so no remote CI result is claimed.

The scoped reviewer found an inherited inaccurate PORTED claim: valid JSON
containing escaped lone surrogates or a 200-deep object falls back to the original
STRING in Rust where the TypeScript result is an OBJECT. Additional actual-source
witnesses were captured first at 5d4492f155. A fourth integration test pins both
refusals and ordinary scalar Unicode/astral/BOM acceptance. The row is now MIXED,
citing the shared CHOSEN UTF-8 representation boundary and separately declaring
the CHOSEN parser-depth bound. This changes the claim, not the numeric repair.
The four final focused tests pass under the isolated compiler. 78 layer tests
and routing equivalence (47 exchanges, 1128 source cases) also pass.
Scoped adversarial reviewer returns MERGE after verifying the inherited-row
correction and its witnesses. Final focused tests and both strict lint targets
pass after that correction; no whole-parser parity claim is made.

## Raw-parser corpus cardinality

This is a test repair; no production decoder behavior changed. The existing
raw-parser oracle exercised three parser entry points over ten source inputs,
but its loop bound came from the capture. Before adding guards, rebuilding with
applied empty (0), shortened (9), and surplus (11) source captures produced
three independent exit-0 runs. The empty capture also satisfied the old
`cases[0].result == null` witness because absent Value indexing yields null.

The repaired target requires ten source cases, walks fixed indices 0 through 9,
and counts thirty executed parser comparisons. A named missing-index assertion
protects the fixed walk independently; a source/native coverage assertion also
rejects surplus source cases. Both whitespace sets must contain 25 entries before
value equality is checked. The original input spellings and normal capture values
are retained; no historical fixture artifact was regenerated.

The permanent child control applies and verifies all three capture mutations,
then requires exit 101 at `HOW-PARSER source case count`. The separate control
runner removes that first guard: empty and subset fail at
`HOW-PARSER missing independently required case`, while extra fails at
`HOW-PARSER source/native case coverage`. Removing all independent cardinality
guards with each mutation still active returns exit 0, establishing that the
new assertions detect a failure the old gate accepted. Restoring the guards
returns all three parser targets to passing, including the pre-existing actual
TypeScript regex-correction control.

This proof covers this ten-case corpus and its independent iteration counts.
It is not a whole-transport audit or a consuming array-deserializer mutation;
those are distinct comparison surfaces and require their own controls.
