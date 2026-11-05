# Shared-socket health and unknown latency

SDK socket producers declare `SocketSpec::public_topics(true)` when the physical
socket also carries direct public topics. Primary channel remains the routing and
authentication role. Hyperliquid/Spot and TxFlow declare this; Lighter and Paradex
retain independent public/private sockets. Shared/disabled public-feed policies
must not claim public traffic on a private socket.

The wasm host counts open, first-frame, last-message and close health in every
carried channel. Reconnecting either carried channel retires the physical socket
once. This describes socket traffic, not per-topic subscription success.

SDK Store latency is `number | null`; null is unmeasured, including after reset.
Native STATUS uses flag 2 for a measured latency, preserving measured zero without
a new section layout. Positive legacy frames retain their numeric sample; old
zero frames mean unknown. Source and frame-fed engines preserve the flag.

NX follow-ups (separate PRs): hide latency when null instead of displaying 0ms;
Requests Log must classify frames on a mixed socket by topic, using this socket
capability only as eligibility, rather than labelling all frames private from its
primary route. No NX source is changed here. No host rollout or deployment is
performed by this change.
