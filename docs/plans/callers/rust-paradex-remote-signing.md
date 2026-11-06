# Paradex remote signing

**Change.** Paradex admits a keyless `remoteSigner` account on the shared
structured ws-auth wire, the same wire as Hyperliquid (#2362) and Lighter.
- Engine carrier: every Paradex signing request is `[Sha256(statement), Sign
  StarkDigest x n]`. The statement is the closed `paradex/1` descriptor
  (`venues/paradex/src/hosted.rs`) projected from the exact native body. The
  origin refuses a projection that does not rebuild the hashes it signs.
  `apply` checks the commitment, then each `r || s` against the Stark public
  key.
- That key is the local provider's seal, or, under a host-answered provider,
  the key bound by the #2363 PublicKey setup (`paradex-public`).
- Device: `hosted_signing` gains `paradex-stark` and `paradex-public`.
  - It binds only its own `paradexAddress` and the network's chain
    (`hosted::network_chain`).
  - It runs `validated_digests` (presence table, ±30 s skew, deadline) at its
    capture clock and returns `0x` hex of the n concatenated `r || s` values.
- TS: `remoteSignerProvider` has a Paradex branch, and the `betaVenues` Paradex
  hard refusal is removed. The construction rule declares `remoteSigning`
  (no PayloadSigner).
- The descriptor create set gains STOP_LOSS_MARKET (native's attached
  stop-loss child).

**Callers.**
- SDK/NX: a keyless Paradex account passes `key` = its Paradex account
  address, plus `paradexAddress`, `subkeyAddress` and `remoteSigner`. The
  browser answering `remoteSigner` runs a keyed Rust Paradex engine whose
  `signWsAuth` serves `paradex-stark` and `paradex-public`.
- A Paradex `payloadSigner` now refuses at SDK construction ("no REST-only
  PayloadSigner lifecycle") rather than at start.

**Obligation.** The host must run `prepareAddressSetup` before start (#2362
does this for any `DelegatedSignerSetup`).

**Deletion prerequisites.** None added. The legacy digest-only Paradex
payloadSigner route stays absent.
