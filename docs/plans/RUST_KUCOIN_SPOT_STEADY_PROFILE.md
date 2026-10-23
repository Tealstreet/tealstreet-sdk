# KuCoin Spot public steady CPU investigation

Owner: codex-9czdjh; requested by claude-ddi2il. Profiling only before a reviewed
profile and follow-on implementation plan; no performance code or merge authority.

## Pinned evidence and objective

Investigate landing `258c880656f9714f14b498e046b8ee52658e702a`. Its SDK and
safe-cex source are byte-identical to `6741c1c6609437e32090c76745a56ca6c2262445`;
the intervening landing change is documentation. Use that actual landing's CI
KuCoin Spot package, run39947696291, artifact ci-wasm-kucoinspot. Verify build.json
sourceHead and JS/WASM hashes before profiling; no local release WASM rebuild.
The shipped optimized module strips names, so anonymous function samples must be
reported honestly rather than assigned guessed Rust symbols.

The same run's strict-replay-kucoinspot artifact contains nine public pairs.
Median steady CPU is TS335.311 ms / Rust302.091 ms: TS/Rust1.10997, just outside
the ten-percent gate. Rust main CPU609.717 ms versus TS1052.121 ms; steady CPU is
the narrow objective. The owner's target is **at least1.20x median**: at the
recorded TS median Rust must reach at most279.426 ms (22.665 ms /7.50% lower).
Local profiles diagnose cost; CI interleaved paired medians decide restoration.
No timing threshold, tape, codec behavior, safety check or gate is relaxed.

## Measurement sequence

1. Profile one actual public replay using the frozen optimized CI artifact and
   Node22 through corepack yarn. Use CPU sampling and retain the JSON result,
   artifact hashes and command. Keep sampling separate from timing pass claims.
   Do not run local replay matrices or load sweeps.
2. Enumerate events and distinguish startup from the last75% steady window used
   by Tape.play. The tape has26,426 WS records:26,094 ticker,326 depth, three ack,
   two pong and one welcome; there are no command records. Correlate sampled
   host/wasm/codec/Store/frame/GC costs with real event counts.
3. Attribute the three requested shared seams using source call paths and
   measurements: #1830 command-entry, #1837 classifier and #1854 execution groups.
   Prove when entry/classification are or are not reached by these events; inspect
   shared per-event sweeps even with empty command/execution registries. Do not
   infer overhead merely from a merge-date correlation.
4. If stripped WASM samples cannot identify a shared callee, supplement with a
   temporary symbolized native diagnostic driver over the recorded payload corpus
   and unchanged landing dependencies. Keep it outside repository source, use two
   Cargo jobs, and distinguish native attribution from browser/WASM timing. Report
   unresolved attribution rather than inventing precision.
5. Send the overseer the committed plan, measured profile, per-event attribution,
   limitations and proposed shared owner/scope before performance code changes.

## Follow-on criteria

Measured engine overhead must be fixed in its shared choke point for every venue,
coordinated with its owner and open PR claims. No KuCoin-local skip, capability or
credential shortcut, or weakening exact delivery, expiry, command retention or
observer completion. A shared fast path needs adversarial empty/nonempty/expired/
queued-group controls and source preservation review. If native codec/Store/host
work dominates instead, report that evidence and request the appropriate scope.
No merge until a new window is granted. Existing venue PRs and Bitunix work stay
separate; their evidence does not establish this performance objective.
