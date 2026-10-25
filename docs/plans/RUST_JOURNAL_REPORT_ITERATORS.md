# Journal report iterator simplification

Owner: codex-ih35js. Engine reviewer: claude-fe7lgm.
Local base: Bitget slice 3, `546f585f187` (tree `965e1d31442`).

This independent second size cut changes only the representation of borrowed
report iterators. The streamed retained refusals are exposed through a slice
iterator (empty when no stream exists), avoiding an Option/FlatMap layer. The
report chain carries borrowed Strings until its final String-to-str mapping,
rather than instantiating that mapping separately in each inner iterator.

The public report and rejection item types stay unchanged. Streamed refusals
precede live terminal refusals; live attempts retain logical-leg ordering.
Historical retries remain silent. Empty reports, UTF-8 text, retained evidence,
refusal counts, accepted IDs and sealed-journal reads keep their current meaning.
No admission, custody, retry, timeout, outcome, compaction or failure-note logic
changes. S1 methods, S2 loops and socket route work are outside this cut.

## Verification and measurement

The additive control mixes a compacted streamed refusal with live out-of-order
callbacks, a recovered retry and KnownPartial evidence. It pins literal report
order, rejection IDs, the refusal count, unchanged journal budgets under repeated
reads, and the terminal's retained accepted ID. Existing report, evidence and
serial stress controls remain required, including the serial-disabled build.

The before artifacts are reused from the exact clean `546f585f187` paired build
for cut 1: Bitget 1,472,027 bytes and Bybit 1,396,213 bytes. They use Rust 1.99.0,
Binaryen 132 and unchanged harness profiles. The after build uses this same
composition, independently of cut 1. Credit only its measured shipped delta;
do not add the two cuts' savings without measuring their union.

## Local result

| Family | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Bitget | 1,472,027 | 1,471,859 | -168 |
| Bybit | 1,396,213 | 1,396,117 | -96 |

Web and Node sizes agree. Raw modules shrink by 581 and 112 bytes respectively.
The production delta received fe7lgm's exact source CLEAR at tree
`6dd56329dadf`. Scoped serial/barrier controls pass 1,155 tests with 10 existing
ignored tests. Serial-disabled controls pass 1,089 tests with the same 10
existing ignored tests. Strict Clippy passes with serial/barriers enabled and
with serial disabled; scoped formatting passes.
The PR opens after slice 3 lands, with actual-tip integration and ratchet checks.
