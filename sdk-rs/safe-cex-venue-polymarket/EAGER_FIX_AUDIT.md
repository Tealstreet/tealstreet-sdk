# Polymarket timing FIX audit

Assignment: `PolymarketAPI.getServerTime`, one row in the coordinator's exact
non-UTF-8 FIX-audit list. The frozen async-wrapper census also identifies
`PolymarketExchange::invoke` / `invoke_unit` as the deferred-JS family.

The historical `public_reads::get_server_time` has no production caller. An
all-workspace Rust reference search finds only its declaration and the two
historical test targets. The coordinator directed deletion of that helper and
its tests rather than acceptance of an unused source copy. All-target compilation
after removal checks that no caller remains. Four other typed public reads are
outside this row and remain unchanged.

The TS manifest row now names the live `binding/members.rs::api_read` branch.
`PolymarketClasses` installs `getServerTime`; `members::class` owns its existing
Scope Promise, eagerly calls the actual xhr member and retains the original
raw data/error. This route already passes the live native probe before repair.
The original 20-case source corpus, three error cases and 512 generated cases
remain byte-unchanged and now execute through this live constructed API, retaining
numeric bits, request URL/method, complete iteration counts, receiver and original
result/rejection identity. Obsolete typed-only refusal tests were deleted with
the unused helper. The class-cost driver now selects the live unit witness and
its real endpoint fault, preserving the existing detection/restoration checks.

The shared SDK boundary did compile two named REDs on the original production
binding: `POLY-SDK-TYPED-PREFIX` observed zero Gamma xhr calls before observer
polling, and `POLY-SDK-UNIT-PREFIX` observed zero leverage warnings. The whole
maintained TypeScript class and DefaultStore, with only terminal xhr held,
independently observe one of each. Both helpers now call the actual member before
boxing the Rust waiter and await that existing Promise. This is the census's
JS-Promise family rule; no extra spawn or per-method scheduler is introduced.
A pending Gamma response still completes the token maps and retains the exact
market metadata object after the SDK observer drops. The receiver is kept alive
throughout that probe; it makes no final-owner-drop claim.

`tests/eager_controls.py` must run alone. It compiles five independent production
faults: typed prefix, unit prefix, timestamp endpoint, raw data alias and original
error alias. Each must reach its named runtime RED, then restore the exact original
bytes and return GREEN. Compiler or setup failures are rejected. Original source
and native receiver/alias guards remain enabled. The new fixture module uses
separate `cfg(test)` and host-target attributes; it is not production inventory.

Readiness requires the full venue suite, all-target strict host Clippy, strict
Wasm library Clippy, fmt, full parity index, override controls, all-features
cancellation and its Node controls, plus CI wiring after retiring the old test
targets. All Cargo invocations use the pinned SDK toolchain, jobs 2 and disabled
incremental compilation.
