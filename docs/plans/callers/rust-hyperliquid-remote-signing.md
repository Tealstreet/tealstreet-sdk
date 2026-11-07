# Hyperliquid structured remote signing

Change: keyless `remoteSigner` Hyperliquid and Hyperliquid Spot accounts are
admitted. Each L1 action's sign work becomes `[sha256(statement), publicAddress,
sign secp256k1Digest]`, where the statement is `{kind: l1, action, nonce,
vaultAddress?, isMainnet}`. The server forwards it on the existing ws-auth
sign-req as `outputFormat: 'hyperliquid-l1'`; the device's Rust WASM signer
re-derives the digest itself, allows only L1 trading actions for its own network
and vault, and returns `0x` hex of `r || s || v`. Startup binds the agent address
once through `outputFormat: 'hyperliquid-address'` with the account address as
`toSign`. No bare digest is ever sent. Local signing bodies are byte-identical.

Callers affected: NX exchange-server (hosted accounts) and the browser signer.

Obligations, in order:

1. The browser must answer ws-auth for a Hyperliquid account with the Rust
   `RustEngineExchange.signWsAuth` of a keyed account (the existing route for
   other Rust hosted families). A TS venue class answers both formats with an
   error, which the server counts like any signer error.
2. Hosts that inject a `signer` into `EngineHost` call `prepareAddressSetup()`
   before `start()` (RustEngineExchange does). It negotiates the engine's
   `publicAddressOwned` setup, answers the PublicAddress task through the signer
   and acknowledges each retirement only after that provider call has settled.
   Without it, an address-bound keyless account refuses every signed action.
3. exchange-server constructs hosted Hyperliquid accounts with `key` (the
   account address), optional `vault`, `remoteSigner`, and no `secret`.
4. Custom `remoteSigner` implementations must sign `hyperliquid-l1` only after
   re-deriving the digest from the statement; never sign a supplied digest.

Deletion prerequisites: none added or cleared.
