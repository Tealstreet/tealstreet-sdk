# WOO heartbeat audit

Assigned rows 11, 17 and 18 concern private ping, public ping and public pong.
The maintained bodies are `woo.ws-private.ts:113–118` and
`woo.ws-public.ts:97–106`. They take no data input. Ping checks disposal, reads
performance.now, writes pingAt and then resolves sendWithLog/private or send/public.
Pong resolves send directly, including when disposal is truthy. Each call ignores
its sender's return value and propagates the original thrown value.

The constructed exchange installs the corresponding `socket_classes` bodies on
its actual public/private receivers. The three typed BaseWebSocket facades had
only test callers, no production binding, registry or FFI callers. Those unused
facade bodies are removed with their exact native facade tests and controls.
The timestamp decoder, public writers, dispatcher and core latency-error test
remain. The archived 40 public and 40 private source heartbeat cases stay in their
original source oracles; they no longer claim native facade coverage.

`heartbeat_binding_tests::heartbeat_members_match_source_order_and_retained_failures`
constructs the actual exchange for 24 profiles before comparing against the
maintained socket classes. It observes disposal truthiness, lexical receiver,
live sender lookup after a setter replacement, signed-zero timestamp bits,
UTF-16 sender payloads and original thrown-object identity from clock/set/send/JSON
failures. The thrown object retains a present undefined field and lone surrogate
units. HTTP and background error reports remain empty in each profile.

This is a dead-facade retirement, not a new runtime algorithm. The same real
binding profiles must pass both before and after removal; no failing-master
behavior claim is made. The all-target compiler checks the remaining callers,
and the full index checks the removal of the three obsolete Rust mappings.
