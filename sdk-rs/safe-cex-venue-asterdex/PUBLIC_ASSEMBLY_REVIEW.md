Read-only sub-agent review covered implementation commit
`f820e03b2ca4c673aa90e1df3a60436a8670ad3a`, following initial review at
`56e748621c292e4ee1fe8057f53a95736280b0a2`. The reviewer ran no gates;
Cargo/extractor/control results are parent-measured in PUBLIC_ASSEMBLY_EVIDENCE.

Resolved findings:

- Null builder rows now raise the source property-access error inside the fetch
  catch, before reaching a later matching row. The named source/native case is
  `check-null-row-before-match`.
- Ticker/mark emissions retain the updated ticker across synchronous Store
  removal. `reentrant-ticker-removal` and `reentrant-mark-removal` assert the
  decoded emitted ticker even after the Store becomes empty. Native events own
  snapshots rather than JS object identities; the row remains MIXED.
- Source topic snapshots clone nested arrays. Later source subscriptions cannot
  rewrite earlier expected snapshots.
- Stream dispatch checks handler presence before symbol assignment/statistics.
- Trade dispatch reads only s/p/q/T/m; unused DTO fields cannot reject a frame.
  Native event String symbol/numeric timestamp remain declared boundaries.
- Fresh `message-numeric-pong` and `message-string-pong` sockets separately pin
  strict numeric -1 routing. The earlier combined witness alone was insufficient
  to reject a swapped guard; that review finding was corrected before controls.
- Form components are percent/plus decoded and signed-query components decoded
  before their native assertions. Raw wire comparisons are deferred until all
  cases in the family complete native assertions; no decode/reencode comparison
  substitutes for model assertions.
- Wallet provider is resolved again for add-chain, accounts and signature calls.
  `switch-provider-replaced` and `approve-provider-replaced` execute the actual
  source workflows with a replacement provider and assert native provider IDs.
- onMessage uses MIXED for the forced model constraints and chosen owned-event
  identity; normalizeAddress now targets the complete wrapper and stays
  divergent for compact native checksum diagnostics.

Development failures were not mutation catches: scoped compilation found eight
initial errors, then a missing SocketFrame::Close arm; targeted test compilation
found missing WsParent/Store signatures and an unused subscriber cleanup token.
The first runnable public tests failed at native topics because the source host
stub omitted BaseWebSocket.isConnected. Adding its readyState getter made all
six targeted workflow tests pass. These uncommitted development runs are not
reported as gates covering the committed implementation.

Enumeration floors: review iteration 3 stopped at the precise follow-up deltas;
source/native scenario iteration 1 stops at 16 public cases and 36 builder cases.
This is bounded behavioral evidence, not exhaustive input-domain equivalence.
