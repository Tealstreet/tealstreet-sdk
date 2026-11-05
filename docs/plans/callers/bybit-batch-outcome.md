# Bybit batch acknowledgement echoes and diagnostics

A split create without caller orderLinkIds may receive server-populated client
IDs. Those unsolicited echoes do not contradict a successful native order ID;
create-batch now follows single-create's rule. If a client ID was requested,
its supplied echo still has to agree. Complete equal result/status counts,
nonempty bounded unique native IDs and exact per-child integer status codes
remain required. Contradictory failure rows carrying native IDs remain Unknown.

Unproven create/cancel errors include the list shapes/counts and up to eight
child summaries: ID field presence, status validity/native code and echo match.
Additional children are counted, not printed. Native/client IDs, response
messages, account values and credentials never enter these summaries. Existing
partial order-ID evidence is preserved in its existing structured field.

The production controls cover two- and three-child reduce-only market closes
with no outgoing client ID and a nonempty server echo. The prior malformed
shape control now supplies a requested client ID for its wrong-echo case;
all of its refusal/publication/no-resend assertions remain unchanged. Separate
controls retain missing status, count, duplicate-native-ID and requested-ID
refusals, and bound diagnostics while rejecting raw identity/message values.

The live XTZ closes fully filled, but no raw ACK was retained; unsolicited
echo rejection is a plausible source-confirmed false-failure path, not a claim
about that missing payload. Batch-cancel cascade acceptance belongs to #3075,
reviewed independently at 146d1aebcae; its matching policy is unchanged here.
Retained confirming reads for genuinely unknown creates are deferred pending
an engine-owned contract; later reads do not rewrite the original write fact.
NX uncertainty copy and position refresh are a separate companion change.
