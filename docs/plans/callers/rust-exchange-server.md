# Native exchange server

**Change.** A new binary crate, `safe-cex-exchange-server` (bin `exchange-server`), at
`sdk-rs/hosts/server`. It serves today's `wss /account` envelope over FrameServer
accounts. Store data is base64 SCXF frames in `msg` op `frame`. `callMethod`
translates into engine commands. Private accounts sign through the client via
`safe-cex-remote-signer`, and routing comes from `safe_cex_native::resolve`.
`safe_cex_native::AccountHandle` gains `capabilities()` and `read()`.

**Callers.**
- The web hosted client (NX `hostedFrameClient`) connects to it at `wss://connect-tyo.tealstreet.io/account`.
- The image is `hosts/server/Dockerfile`.

**Algos.** chaser, TWAP and exec run per private account through an
`AccountPort` over `AccountHandle`. Disposal runs in order: TWAP `terminate_all`,
then chaser `shutdown`, then the scope, then the engine. Store fills are not yet
projected: `Change.fills` is empty.

**Not yet wired.** Shared public feeds and init pacing.
