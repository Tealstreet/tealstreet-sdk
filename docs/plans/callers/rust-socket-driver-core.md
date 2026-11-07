# Concrete managed socket core

Managed socket Driver and Supervisor transitions use the engine's borrowed
`CoreCx`, so their venue-independent state, timer and effect bookkeeping is
compiled once. Public `Cx<P>` and Supervisor APIs remain typed wrappers;
`Engine<V>` retains typed proposals, signing continuations and venue callbacks.
There is no new allocation, boxed continuation or indirect callback.

Retirement still cancels typed continuations, releases deferred frames, records
health and closes transport before notifying the venue, then acknowledges the
retirement and schedules recovery. Capacity exhaustion retains retirement
effects before dropping typed proposals, then publishes the existing notice and
stops feeds. Moving either typed destruction boundary changes observable action
and destructor ordering.

The allocation control in `socket_auth_progress` pins the pre-extraction budget
for startup, open, auth progress, accepted auth, ordinary data and expiry. The
managed venue socket/supervision suites retain their native wire assertions.
No venue, NX, server or host caller changes are required.
