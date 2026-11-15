# Native remote signer

`remote_account` installs opaque identities and forwards signing work over the
existing device `sign-req` / `sign-res` channel. Credentials and private signer
setup are removed from engine options before construction.

Public mode is resolved before custody validation or reference installation.
Explicit public requests, including captured `publicMode.requested`, return
empty signing identities and an inert provider without a device link. KuCoin
Futures/Spot accounts without a public key also resolve to this public capsule.
Captured intent takes precedence over the legacy `publicDataOnly` flag. Unused
credential/setup fields are stripped, and all signing capabilities refuse.

Private KuCoin Futures and Spot use the existing `kucoin-batch` device format.
Private setup installs a passphrase reference; its value stays with the device and is
neither required nor retained by this host. Complete public `extra.kucoinBroker`
partner/name metadata also installs an optional broker reference. A supplied
`brokerKey` is stripped from engine options; the device resolves its own key.
Native secondary remote credentials remain unsupported.

The provider admits the closed atomic graph: present the owned public credential,
HMAC the exact byte preimage, HMAC the owned passphrase credential, and optionally
HMAC the public broker prefix plus the owned public credential. The descriptor
carries only `{slot: "primary", payload, brokerPrefix?}`. Neither private material
nor opaque references cross the device wire. Credential-only presentation stays
local; partial graphs and raw passphrase presentation refuse before device I/O.

One reply must contain the selected public credential and exactly two or three
canonical base64 SHA-256 MACs. A wrong account, malformed MAC or partial reply
refuses the entire batch; no fallback or partial result is published. The engine
receipt remains attached to the original graph.

Run `cargo test --locked -j2 -p safe-cex-remote-signer` from `sdk-rs` for native
setup/provider/link tests. KuCoin tests drive the actual registry device codec
and compare every result byte with an independent LocalSigner control.
