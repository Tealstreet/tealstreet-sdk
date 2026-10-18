# SDK initialization scheduling

Owner: codex-kxakrb. Assigned row: `initSdk`, from the exact lane assignment
linked by `non-utf8-assignment-index.json`.

TypeScript `zklink/signer/utils.ts:8` invokes the imported WASM initializer when
its module flag is false, awaits it, then sets the flag. Repeated calls reuse
that initialized WASM module; concurrent first calls are not coalesced.

The production native binding installs `binding::signer::NativeSigner`, whose
`ZkLinkHost::init` is an immediately ready successful unit future. The native
cryptographic implementation is compiled into the binary. There is no module
fetch, external initialization request, signer mutation, or timer behind that
await. `SdkInitialization` still writes its flag only after successful completion.
The broader behavior of arbitrary user-implemented `ZkLinkHost` futures is outside
this constructed native binding observation.

Probe: `binding::native_sdk_initialization_has_no_deferred_module_work` uses the
actual constructed exchange, API interceptors, initializer, seed-derived L2
signer and Store. Only terminal HTTP/socket/runtime primitives are supplied. It
validates twice and places two distinct client-ID orders, inspecting real wire
signatures and authentication fields. Its bounded driver only pumps runnable
jobs; it never advances time. The control inserts a pending await into
the production initializer, requires that named compiled binding test to fail,
and reruns it after exact source restoration.

Verdict: ACCEPT for the constructed native binding. The changed scheduler
mechanism cannot postpone any SDK module-loading effect: there is none in this
implementation, and both cold and repeated signing finish without timer progress.
The positive probe and independently compiled pending-initializer refusal both
pass their expected checks; exact restoration passes again. No scheduler change
or custom-host parity claim is proposed.
