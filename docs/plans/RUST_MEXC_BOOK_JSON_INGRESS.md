# MEXC standalone REST-book command admission

Fresh landing: `9a796e15f1a6e4ca93c85d2fd034b8cf8ff6c292`.
MEXC already advertises and implements REST FetchOrderBook, but its standalone
JSON decoder admits that op only when unrelated AsterDEX/Polymarket features
are unified in. Combined/default tests mask the public API refusal.

1. Reproduce standalone actual JSON decoding failure, using the existing MEXC
   book request path and native price/quantity/result expectations as controls.
2. Depend on the precise command-order-book feature introduced by reviewed
   Backpack #1680 head `cb28b85d74d195536d4828e4c2eac6d8e0a899fd`.
   Integrate only that genuine shared dependency; opt MEXC into the feature via
   its engine dependency. Preserve every existing book mapper, request, rate,
   market validation, auth, signing, limit and capability behavior.
3. Regress JSON to actual unsigned native HTTP and typed result/error ownership.
   Audit all already-advertised book families: AsterDEX/Polymarket retain their
   explicit decoder features; newly enabled families must opt in precisely.
4. Independent review; MEXC+Binance native tests, strict touched-crate Clippy,
   format/bans and CI. Merge only after the exact reviewed Backpack predecessor
   actually lands, or integrate actual landing and rerun if its head changes.
   A fresh separate post-predecessor guard must list only this PR's own files.

No new book semantics, budget changes, unrelated venue actions or local release
WASM/replay matrices. Four local Cargo jobs; report the actual merge pin.

Verification:
- The standalone regression fails on original actual landing at JSON decoding
  (unknown fetchOrderBook), before any HTTP; it passes after the precise opt-in.
- Genuine dependency merge contains exact reviewed Backpack head cb28b85d74d.
  All MEXC runtime source files remain identical to that dependency; only its
  Cargo decoder admission changes. Existing AsterDEX/Polymarket features retain
  admission; no other previously advertised family shares this omission.
- All 391 MEXC+Binance native tests, strict MEXC all-target Clippy, format and bans
  pass. Two independent correctness/preservation reviews are clear.
- Exact-head full CI and actual predecessor landing remain required. The final
  post-predecessor guard must list exactly the six owned files and no Backpack
  or previously landed caller documentation changes.

CI isolation refinement:
- Native workspace feature unification can hide later removal of a family's
  decoder opt-in even though the targeted regression proves the defect locally.
  The existing family artifact matrix now probes actual JSON fetchOrderBook for
  every successfully constructed family instance advertising the capability.
  Returned sans-I/O requests are never dispatched. This catches the decoder
  omission in isolated shipped artifacts without local release builds or timing.
- The final own scope is six files including this existing harness matrix test;
  all runtime mapper/error/capability semantics remain unchanged.
