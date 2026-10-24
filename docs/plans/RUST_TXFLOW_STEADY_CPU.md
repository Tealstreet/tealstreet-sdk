# TxFlow steady ticker CPU

Base: actual `77f124089477a6aed502d9b02aa7f08752b7bfa5`. Owner: codex-5c57jl.
The request owner hcmq3f confirmed no overlapping TxFlow source claim.

## Measured target

Profile the public tape's steady interval, starting at its existing first-quarter
warm mark, with named WASM and the unchanged build settings. Separate profiles
from the uninstrumented before/after CPU samples. The tape contains 525 socket
messages, including 417 ticker updates. Initial steady samples include native
`on_message`, ticker Store lookup and allocation work. No profile, workload,
threshold, transport or socket authority change is part of this fix.

## Change

Reuse the catalogue instrument's borrowed symbol instead of allocating its name
on every ticker. Cache each instrument's ticker row after the existing complete
seed commits. This is a row-location hint, not publication or lifetime authority:
check the current Store row's symbol before using the index. A miss retains
`Store::update_ticker` and its existing missing-seed recovery rules. Metadata
replacement starts with empty hints; the subsequent seed rebuilds them. The
hint is skipped by metadata deserialization and does not alter venue wire data.

Keep the numeric decoder, ticker masks, live-before-seed overlay, all reducers,
HTTP builders, account hooks, socket checks and startup sequence intact. There
is no new shared engine API or venue lifecycle state.

## Verification and measurement

- Exercise real current-socket ticker updates, missing and unknown identities,
  table replacement and the existing late-seed/catalogue rejection cases. Preserve existing
  financial, public-policy, retirement, expiry and supervision controls.
- Build baseline and candidate from the same base with pinned Rust, bindgen and
  Binaryen; only the candidate source differs. Run nine alternating fresh-process
  TS/baseline/candidate triplets and retain every sample. Compare complete replay
  outputs excluding metrics and report CPU, memory and shipped size separately.
- These local Mac measurements diagnose the change; full CI retains the existing
  Linux CPU, memory, correctness and size gates. No favorable-run selection or
  threshold adjustment is allowed.
- Run scoped native tests, strict Clippy, fmt and normal bans; open a draft PR.

Results and exact source review receipts will be appended after execution.


## Candidate receipt

Source `868915c2797b76d6ff42eeebd2fcf72dac0aaaae` changes only native ticker
projection and its metadata row hint, plus two dedicated controls. All 90
existing scoped account/public/managed socket/supervision controls pass, and the
new wrong-row/unknown-ID and loaded-missing-row controls both pass. Strict
Clippy, fmt and normal bans pass. The engine, host, HTTP/account reducers,
request/signing/socket manifests, build profiles and thresholds are unchanged.

Nine alternating fresh-process triplets on mbp2 (arm64, Node 22.23.3), using
unchanged Rust 1.99.0/bindgen 0.2.129/Binaryen 132 family flags:

| Median | TS | Baseline | Candidate |
| --- | ---: | ---: | ---: |
| Steady CPU, ms | 14.855 | 11.527 | 10.931 |
| Main-thread CPU, ms | 430.985 | 109.680 | 104.989 |
| Process CPU, ms | 647.370 | 179.390 | 173.532 |
| Retained bytes | 1,283,312 | 764,160 | 764,160 |

All complete baseline/candidate output excluding metrics is identical; every
arm's output is stable across all nine runs. The observed steady median falls
0.596 ms (5.2%), but the shared machine was noisy: baseline spans 7.876–26.992 ms
and candidate 8.009–24.049 ms. Keep all samples; this does not certify Linux CI
margin or establish the whole median difference as causal.

An independent same-process ABBA ticker probe uses two genuinely opened
engines and their real `wsMessageBytes` entrypoints, after 10,000 warm-up calls
per arm. Eight ABBA rounds retain sixteen 10,000-event blocks per arm. Median
cost is 1.8235 to 1.7096 microseconds/event (6.2% lower); neither engine fails or
emits unexpected actions. This probe includes ingress/drain but excludes varied
payloads, frame flushing and the tape's timer mix; it is not the gate workload.

Node WASM grows from 1,083,572 to 1,084,212 bytes (+640), inside the unchanged
1,343,488-byte ceiling. Raw runs, normal and named artifacts, profiles and logs
are under `/tmp/codex-txflow-steady-study/`. Full CI and Linux CPU/memory gates
remain pending; no performance gate, tape or assertion is weakened.
