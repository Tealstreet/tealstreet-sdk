# Bitunix synchronized host clock

Clock policy supersession: [engine clock standardisation](RUST_ENGINE_CLOCK.md)
replaces the historical per-venue `hostClock` opt-in below. Production SDK
engines share one synchronized default; explicit host/tape clocks remain
authoritative. Original implementation/regression receipts are retained.

Bitunix signs REST requests and private login using the engine's `cx.now`, but
its default capability record leaves `hostClock` false. The SDK therefore feeds
local wall time instead of its synchronized virtual clock, allowing local clock
skew to reject otherwise valid authentication.

Override only `Capabilities.host_clock` for Bitunix. Update the native combined
host's expected record and the SDK fake-engine record together. Preserve every
other default capability and all existing signing bytes/vectors; no signature
retry, server-time endpoint or write resend is introduced.

Add a native capability regression plus actual REST/login timestamp checks with
explicit engine times. Add a bridge regression with divergent local/virtual time
that checks the very first engine start and later socket-open callback, proving
that the capability is adopted before startup. Include a false-capability control
so the test verifies record-driven behavior. Run the new assertion against the
original record first, then Bitunix/Binance native, relevant combined host test,
focused SDK tests/typecheck/lint and strict Clippy/fmt/bans. Independent plan and
implementation review, green CI and separate exact guard precede merge. This PR
follows the pending Bitunix source queue and integrates actual preceding landing.

The audit's separate signature-rejection resend and venue server-sync protocol
remain owner decisions; this phase closes only synchronized SDK host-time input.

Plan and implementation independent reviews CLEAR. The original native record
fails the full-capability assertion and the original SDK fixture passes a clock
skewed by ten minutes (false-capability control passes). With the correction,
both real native timestamp checks and both SDK controls pass. Bitunix/Binance
native, the default combined-host capability fixture, 22 SDK clock/capability
tests, package typecheck/lint and strict Clippy/fmt/bans pass.
