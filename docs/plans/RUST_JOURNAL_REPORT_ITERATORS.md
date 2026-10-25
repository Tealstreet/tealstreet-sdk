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


## Actual slice-3 landing composition

Plain-merge of actual `ae3f8b47cc1` into the reviewed independent cut gives
source `3a04f7ce924`. Source merges automatically; only README keeps incoming
rows plus this plan row once. The engine delta is byte-identical to the
previously reviewed cut, and fe7lgm carried exact-patch CLEAR.

| Family | Before on ae3 | After on the same composition | Delta |
| --- | ---: | ---: | ---: |
| Bitget | 1,475,547 | 1,475,379 | -168 |
| Bybit | 1,397,889 | 1,397,793 | -96 |

Rust 1.99.0, Binaryen 132, unchanged profiles and Web/Node agreement. Raw modules
shrink by 581 and 112 bytes respectively. No allowance change and no sum with
cut 1 until their union is measured.

Fresh composed engine rows pass 1,230 serial/condition/barrier/nonce, 1,155
declared/nonce, and 1,167 declared/barrier/nonce tests, retaining ten existing
ignored tests each. Strict all-target Clippy passes all three rows; scoped
formatting passes. The new report-control name originally contained the token
for a forbidden JSON feature, despite testing unrelated report ordering. Only
its name is corrected; its body and production source remain unchanged. The
renamed control and formatting pass again, followed by full bans/ratchets.


## Resumed landing-tip verification

The plain merge of actual `34999f973f9` gives source `86b14d8fd2c`; README is
incoming plus this plan row once, with no source conflicts. The own production
and control bodies are unchanged. The incoming engine change is the separately
landed socket repair helper, outside this cut's report code.

A fresh same-tip pair again measures Bitget 1,475,547 -> 1,475,379 (-168) and
Bybit 1,397,889 -> 1,397,793 (-96), with Web/Node agreement and unchanged
Rust 1.99.0/Binaryen 132 profiles. All four shipped artifacts are byte-identical
to their earlier same-source measurements; credit remains the measured result.
The completed measurement target is removed, retaining artifacts and receipts.

Focused report controls pass 7/7 with serial/condition/barrier/nonce features
and 6/6 with declared/nonce features. Strict all-target Clippy passes both
compositions; scoped formatting and full bans/ratchets pass. The earlier full
feature-matrix evidence remains recorded above. This integration enables the
PR-triggered Rust workflow previously blocked by the README conflict.

Full Rust run 40211927978 (74 success, three configured skips) and general
run 40211928021 are green on `231cd805454`. The subsequent README-only fix
keeps incoming bytes plus one own row. Plain-merging `094a1fb836f` adds only
the landed AsterDEX and repair-plan changes; the two own SDK blobs remain
byte-exact to that green head. The measured Bitget/Bybit inputs are unchanged.

Cut 1 landed as `35ae6c6b254`. This independent cut now plain-merges that
actual landing, retaining its schedule change and the incoming README rows.
Only the report iterator production delta and its unchanged control remain
own SDK changes; no independent savings are summed into a combined claim.
