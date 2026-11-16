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
| `bybitinverse` | Native inverse private startup, HMAC valid/wrong-key/replay controls and Brazil-site session suite |
| `bybitdemo`, `bybitspot` | Native demo/spot private startup and shared HMAC/session negatives; exact origin and category checks |
| `binance` | Native private startup and shared HMAC suite |
| `binancepm`, `binanceinverse`, `binancespot` | Native PAPI/DAPI/spot private startup and shared HMAC/session negatives; exact product routes |
| `okx` | Native private startup and shared HMAC suite |
| `okxdemo` | Native demo startup and shared HMAC/session negatives; exact origin and simulated-trading headers |
| `okxeu` | Signed config before keyed discovery, full native EEA startup, bounded signature-refusal retry and shared HMAC/session negatives |
| `okxeudemo` | Exact EEA demo startup with one simulated-trading header per private read, bounded signature-refusal retry and fresh connection proof |
| `bitget` | Native private startup and shared HMAC suite |
| `bitgetusdc` | Native USDC private startup and shared HMAC/session negatives; exact product type |
| `blofin` | Native private startup and shared HMAC suite |
| `kucoin` | Native private startup and shared HMAC suite; mainnet only |
| `asterdex` V1 only | Native HMAC private startup on mainnet/testnet, actual device batch codec and connection-bound negative controls |
| `mexc` | Native signed position-mode/account reads and private-stream HMAC; wrong-key, replay and socket/Session controls; mainnet only |

`okxeudemo` has its own exact catalog entry and demo-header/private-startup
controls; EEA mainnet readiness alone never admits it.
EEA bootstrap retains the original config authority until catalogue completion;
config success alone cannot satisfy private readiness. A second consecutive
`50113` for a bootstrap read refuses permanently after one re-signed retry,
without extending the captured admission deadline.

Only these exact names are admitted; the catalog never matches a family prefix.
AsterDEX additionally checks the init shape: address-prefixed keys or nonempty
main-wallet application IDs remain unavailable. Only its V1 API-key shape gets
the native `asterdex.apiVersion: "v1"` setting. The accounting ledger explicitly
excludes the V3 wallet shape; V1 readiness is not wallet possession proof.
An unsupported proof returns the existing account error envelope with
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

A pre-probe registry compatibility refusal uses the existing `msg`/`error`
envelope with `data[1].initRefused: true`, `kind: "signing-mode"` and
`transient: false`. Its message is "The requested account signing mode is
unavailable; account not attached". This class does not assert that a key was
tested or found invalid. Hosts must preserve the class, show the opaque message,
and keep that registration unadmitted; a fresh handover retry is not a remedy.
The reason stays off the socket. Existing custody-mode, identity and holder
checks still apply, and this response does not authorize replacing an account
or replaying a write.
