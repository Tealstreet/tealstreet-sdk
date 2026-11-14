# COSIGNED join proof

A fresh private COSIGNED connection completes the existing pending-join probe
before it attaches to an account, becomes its signer or receives account state.
The server builds native private-startup requests on the account's existing
route. The joining device returns signatures through the existing `sign-req` /
`sign-res` wire; it does not provide the request or a verification result.

COSIGNED proof readiness is independent of DELEGATED hosted eligibility.
The exact proof-ready venue names are:

| Venue | Probe |
| --- | --- |
| `bybit` | Native private startup, existing Bybit session suite |
| `binance` | Native private startup and shared HMAC suite |
| `okx` | Native private startup and shared HMAC suite |
| `bitget` | Native private startup and shared HMAC suite |
| `blofin` | Native private startup and shared HMAC suite |
| `kucoin` | Native private startup and shared HMAC suite; mainnet only |

These names do not imply readiness for sibling spot, inverse, regional or demo
variants. An unsupported proof returns the existing account error envelope with
`initRefused: true`, `kind: "proof-unavailable"`, `transient: false` and
`outcome: "notSent"`. The server attaches no holder, starts no signing request or
probe, and leaves an existing resident account intact. An unknown or retired
venue still uses its existing unsupported-venue refusal.

Hosts must surface this refusal and keep the account unadmitted. They must not
interpret silence, an existing resident account or a matching settings
fingerprint as successful verification. A new proof family needs positive,
different-key, previous-attempt-signature and consumed-response coverage before
it enters the proof-ready catalog. Public-key family helpers alone do not make
a venue ready.

During verification, method, snapshot and algo requests are refused as
`verifying`, not queued. Invalid credentials return the existing non-transient
`unverified` refusal; network/rate-limit/timeout uncertainty returns its transient
variant. The candidate proof never changes the resident signer on failure.
Successful same-connection matching inits refresh the existing snapshot, and
same-user COSIGNED resumes within the existing 60-second window keep the current
resume contract. Token expiry, lease authority, output retirement and captured
connection/generation checks continue to apply.

DELEGATED retains the generic hosted eligibility rule, its handover and local
signer. KuCoin mainnet now has a configured Futures origin and qualifies under
that rule; its private-startup proof is still required. Account identity,
retention and socket protocol version are unchanged, and no refusal grants
permission to retry an interrupted write.
