# NaN capture audit

Capture host for this sweep: darwin arm64, Node v24.20.0, Rust host arm64.
No x86-64 execution is claimed. Live captures must use the running architecture;
there is no frozen CPU-specific NaN payload contract. Original archive bytes and
shared feeder artifacts are not regenerated.

| Bit-comparison family | Input/output examined | NaN in maintained corpus | Action |
| --- | --- | --- | --- |
| ticker-events oracle + Rust native Store field comparison | Execute all 209 cases; inspect each field's 64-bit exponent and mantissa | 80 captures across 11 fields | Canonicalize at JS capture and Rust comparison to 7ff8000000000000, nothing else |
| response-phase oracle | Fourteen parsed JSON response profiles; tagged typeof/numberBits | None: numbers are parsed 123, JSON numbers; JSON.parse cannot yield NaN on these bytes | No change |
| OKX numeric ingress | Enumerated raw/owned/borrowed/nested/tagged mark lexemes and native values | None: finite, negative zero, positive/negative overflow and underflow; no NaN lexeme or computed NaN | No change |
| OKX exchange policy bit witnesses | Explicit -0, -0.0, Infinity, 1.25 and Unicode positive marker 7 | None | No change |
| cache numeric ingress | Seven fields times four enumerated lexemes times three paths | None: -0, -0.0, 1.25, 1e400 | No change; existing signed-zero finding retained |
| owner Request bit witnesses | Four fields with native Value(-0.0) | None | No change |
| cache calendar numeric oracle | Reviewed equal_number path and split endpoints | NaN results already compare is_nan on both sides; split endpoints tested finite | No sign-bit snapshot; no weakening of finite/-0 comparison |

The ticker oracle and Rust test are consumed only by the OKX gate: reference
inventory found no external oracle importing their emitted corpus. Its live
capture can therefore be repaired in place. It does not supply the shared numeric
corpus. The original 209 cases remain, and NaN alone changes spelling.

Named permanent capture controls: `capture signed zero identity` refuses a
negative-zero-to-positive-zero mutation; `capture finite payload identity` refuses
finite-to-zero capture. Both children exit 1 at their named AssertionError; restored
capture and Rust Store bit comparison pass. The controls compare literal expected
bits rather than deriving both sides through the helper under test.

This audit concerns maintained inputs and captures, not a claim that arbitrary
caller-provided numeric models cannot contain NaN. Value/JSON normalization is not
used to infer absence: the inputs above and executed pre-serialization bit tags
are the evidence. The fixture identity census has a separate limitation: it counts
emitted JSON objects and cannot recover NaN or null erased before emission.

Native comparison controls also protect the comparison half, not just capture:
`comparison signed zero identity` rejects a compiling comparator that treats both
zeros as equal; `comparison finite payload identity` rejects a compiling comparator
that accepts all nonzero finite payloads without comparison. The same comparator
is called by each live Rust Store field assertion. Both wrong comparators compile
and must fail at101, then restored comparison and live captures pass.

Added older numeric-helper supplemental family: 140 cases across five helpers capture native-result bits and presence. Its finite guards turn non-finite parsed values into zero (optional helper omits them), so the maintained results contain no NaN. Negative zero is retained for string -0; native String(-0) semantics are a separate positive-zero result. Capture still canonicalizes only a future NaN, never finite or zero bits. Architecture baseline remains darwin arm64/Node24.20. Original corpus bytes remain unchanged.

Ping clock corpus: twenty live cases draw only from five finite clock values,
including positive/negative zero and fractional values; no NaN can enter that
closed input generator. Each capture records architecture and Node major.
Assertions read effect order, exact errors and native pong latency rather than
re-encoded bytes. Disposal and failure flags cover all four combinations, so
identical booleans on two cases are not the only field-identity evidence. This
new live corpus feeds no gate outside OKX and rewrites no original artifact.
