# Paradex perpetuals

The sans-I/O venue names the real production/testnet REST and WebSocket URLs.
Use the SDK's resolved Tealstreet transport in JavaScript, or pass its serialized
routing plan and proxy-token callback to the native host. Paradex proxy requests
carry `Tealstreet-Authorization` separately from the venue's `Authorization`
JWT. The current routing table declares no Paradex WebSocket relay override.

WebSockets require SBE schema 1, version 2. Public and private sockets share an
upstream URL but have separate channel roles, authentication and recovery.
Hosts must support the explicit binary transport path; no JSON-only fallback
is available from the current upstream service.

Private mode supports local StarkNet subkeys and the SDK's Ethereum-key account
derivation. It needs the Paradex account/subkey addresses and system configuration
for the selected environment. Payload signing uses host signature actions for
JWT, placements and amendments; each write waits for its correlated signature.
Public-only mode needs no credentials. The SDK registration is opt-in; its shared
default loader initializes generated WASM assets for the current realm. Default
runtime retirement remains a separate landing phase.

Orders support limits, market orders, capped market IOC limits, conditional and
bracket orders, native amendments, and per-market cross leverage. Fixed cross
margin and one-way mode requests complete without a write; isolated/hedged mode
requests reject. Failed or ambiguous writes are never automatically replayed.
HTTP 429, 408, and 5xx after dispatch explicitly report an unknown outcome and
ask the caller to Check Open Orders. Native refusal text on these responses is
diagnostic evidence only and cannot authorize a protective put-back.
Transport loss and unusable write acknowledgements carry the same warning before
aggregation, while preserving the original diagnostic separately.
Joined failures retain the first uncertain leg's structured diagnostics and
classification alongside the public messages from every failed leg.
Serial placement entries continue after a definite refusal, preserving accepted
IDs and reporting failed client identities at Error level. A failed native batch
stops remaining placement dispatch and fails the command, retaining accepted IDs
in the failure's partial outcome. Refused entries skip their attached children;
a refused TP skips its SL, while the next independent entry can still be sent.
An Unknown placement halts all remaining writes for that action, the approved
stop-after-UNKNOWN parity exception. Its failure includes
"The outcome is unknown. Check Open Orders", accepted IDs and uncertain placements.
The `order.outcome_unknown` event records the existing outcome journal and
withheld writes; native diagnostics remain separate. No uncertain write is
replayed. Leverage requests carry a JSON integer.
After an amendment, another amendment requires coherent native order evidence;
an old replica response cannot restore the previous signed price, size or trigger.

`docs/plans/RUST_PARADEX.md` records phase reviews and replay gates. The harness
verifies emitted authentication/order signatures independently and replays
recorded public binary frames without replacing market data.
Malformed public SBE frames log and drop without closing the socket or clearing
books. Valid sequence gaps still resubscribe only the affected book, and malformed
traffic does not refresh the silence watchdog.

Public market discovery retries until it recovers, with jittered exponential
backoff capped at 30 seconds. After two failed attempts, its one-time Error
includes the latest cause and bounded `firstCause`/`cause` fields, distinguishing
transport refusals, HTTP status failures and unusable lists. Response bodies are
not copied into that diagnostic. Valid perpetual rows can load despite malformed
peers; an empty or rejected catalogue keeps any last-good markets and retries.
