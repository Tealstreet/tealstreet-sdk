# KuCoin provider cutover

This is stack B, clearing the unused-prerequisite hold in
`kucoin-custody-dispatch.md` when both PRs land back to back. Hosted admission,
remote/payload bridges, and live requests remain outside this change.

SDK credential inputs stay the same. Trusted native construction must use the
registry builder, or install its returned provider and identities together.
Low-level venue Options now accept opaque identities and public broker labels;
they cannot carry an API secret, raw passphrase, or broker key. Native/WASM
setup consumes raw fields before constructing the venue. Malformed raw setup
refuses during construction; the venue still rejects malformed opaque refs.

The same selected key signs both request and passphrase bytes with UTF-8
HMAC-SHA256. Broker signing stays optional and aliases the primary's one role
ref into the read slot. Existing GET/read-key selection is preserved, while
shared Selection binds its egress. Refused write signing stays NotSent, with
no unsigned fallback, and the next valid write can proceed.

This adopts custody and dispatch only. The request inventory also shrinks
through three duplicate issue sites and three proven numeric-format macros;
the remaining legacy HTTP lifecycle is not claimed as managed adoption.
Remaining signing rows are test oracles. No test inventory growth is allowed.
