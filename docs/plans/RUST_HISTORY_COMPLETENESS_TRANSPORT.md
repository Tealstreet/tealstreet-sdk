# SDK history completeness across request transports

Base: `391dbe9af87e` (landing after #1661). Re-verify B11 in
`RUST_CUTOVER_CALLERS.md`, then fix SDK-owned propagation. NX stays read-only;
its adapters and historical coverage changes remain documented caller work.

1. Commit this short plan before implementation. Current gaps: the JS host
   marks a command's array non-enumerably, but CallMethodHandler sends it
   directly and BaseExchange resolves the cloned result without restoration.
   MCP withUtc mapping and JSON serialization also discard it. Rust SDK
   reported/derived history mappings can lose it before these transports.
2. Add one shared typed helper for an optional serializable completeness
   record `{ incomplete: true }`, reading only explicit marked arrays and
   restoring the existing non-enumerable array flag. No marker means no
   assertion of exhaustiveness; never manufacture a complete status.
3. Add optional completeness beside `requestId`, `result` and `success` on
   successful worker replies. Restore it only inside the matching pending
   request's success handling. Exercise both forwarded and ordinary history
   branches, structured clone and JSON/hosted hops, concurrent flagged and
   unflagged requests, late replies/timeouts, failures and malformed metadata.
   Keep errors and request lifecycle ownership intact; no global warning latch.
4. Preserve flags through SDK-owned Rust history transformations (including
   multi-axis Bybit derivation) and MCP UTC mapping. Serialize flagged MCP
   history explicitly as rows plus a completeness record associated with its
   JSON-RPC request; retain unflagged tool response behavior. Keep terminal
   failures, available partial rows and absence of completeness evidence distinct.
5. Add regressions that fail before implementation. Run focused SDK/MCP tests,
   package typechecks/lint and existing compiled consumer CI. No local release
   builds, full replay sweeps or new runtime/deployment modes. Engine/venue
   paging limits, financial calculations, signing and history failure policies
   are outside this metadata propagation change.
6. Update the B11 caller document to distinguish implemented SDK boundaries
   from NX-owned VWC/shim field forwarding, derived axes and coverage tracking.
   Coordinate the exact additive API with codex-cxxyqr; no NX edits or deploys.
   Independently review correctness/adversarial cases, fix findings, integrate
   landing under lane rules, require green CI, run a fresh separate revert
   guard, merge the exact head and report the actual pin to claude-ddi2il.
