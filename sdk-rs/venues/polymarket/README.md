# Polymarket signing

The connector uses CLOB V2 on Polygon (chain 137): EIP-712 wallet signatures
for L1 credential derivation and orders, and URL-safe base64 HMAC-SHA256 over
`seconds + method + path + exact body` for L2 REST authentication. Query strings
are excluded from the L2 preimage. Deposit wallets (signature type 3) use the
ERC-7739 nested order signature; proxy and Safe accounts retain types 1 and 2.

Hosted `RemoteSigner` and delegated `PayloadSigner` remain explicitly disabled
in `construction-rules.json`. The private user WebSocket authenticates with
`{apiKey, secret, passphrase}`: it requires the actual L2 API secret, not a
signature. A signature-only callback cannot construct this subscription while
keeping secret material on the device. Wallet signing and L2 HMAC also use
different key types; enabling just one would not provide private-account
parity. SDK and native construction refuse these modes with `unsupported` /
`notSent` before credentials are decoded, callbacks run or transport starts.
Hosted construction also enforces SDK beta admission: Polymarket stays OUT
for both public and private accounts there. Client construction remains
available behind the SDK beta layer; private delegated custody still refuses.
Independent venue codecs and their signing vectors remain tested.

The existing local connector supports its wallet/proxy/Safe/deposit-wallet
account types. Those account forms do not substitute for the user-channel
credential. The beta venue admission list is unchanged. Supporting hosted
accounts requires a separately designed credential/session transport (or a
venue-supported signature-based user stream), not exporting the API secret in
a signer payload or result.

Protocol references:
- [Authentication](https://docs.polymarket.com/getting-started/api)
- [User stream](https://docs.polymarket.com/trading/realtime-order-updates)
- [CLOB client V2](https://github.com/Polymarket/clob-client-v2)

The local signing module retains independent official-client golden vectors
for L1, L2 and all four order signature types. No remote signing vector or WS
signature is claimed for a mode that cannot authenticate this protocol.

Order sizing follows the legacy TS connector at
`packages/safe-cex/src/__tests__/reference/polymarket/polymarket.exchange.ts:660`:
`precision.amount` counts decimal digits for `Number(amount.toFixed(...))`,
followed by the official builder's share rounding. Each input produces one
payload, including sizes above the market maximum; the venue supplies any
refusal. Minimum-size and supported-order checks remain before dispatch.
KNOWN-ISSUE: TS sends above-max; venue refuses. Decimal rounding can also
under-allocate; the exact TS result is retained for exact parity per Sam's decision.

Unknown write outcomes retain the native diagnostic and use `The outcome is unknown. Check Open Orders`.
The passive indexed journal preserves accepted orders, the uncertain element and
unsent suffix; the existing driver stops without retrying. A single attributed
`CommandError` and `order.outcome_unknown` event accompany the terminal, without
logging signed bodies or credentials. Definite native refusals remain refusals.
