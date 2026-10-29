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

**Not yet wired.** chaser, TWAP and exec (their `msg` ops reply "not supported yet"), shared
public feeds, and init pacing.
