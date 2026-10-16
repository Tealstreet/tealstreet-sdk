# Legacy WOO socket control evidence

Code/gate SHA: `f6eb508d3de31de0ed16e5b9c9d554bcdb942108`.
Actual parent: `8b3442af01690851ec63406f14e7cca587959060`.
Fresh branch: `feat/rust-woo-legacy-socket-control`. No earlier unreviewed feature
work was used as an ancestor. The preceding metadata slice was already merged.

Python 3.11 tomllib census over the three owned legacy manifests independently
expects **101 rows**. Parent: **22 ported / 20 divergent / 59 pending**.
Code commit: **30 / 20 / 51**. Exactly **8 pending -> ported, 0 -> divergent**:
connectAndSubscribe, onOpen, subscribe and onMessage in each legacy socket.
Only woo.ws-public.ts.toml and woo.ws-private.ts.toml change; socket-class and
data-handler rows remain separately pending. No v3-family file/row is edited.

Native targets reside in src/legacy_socket_control.rs. These implement the eight
named method bodies over compatible Host primitives, not completed socket
classes or runtime registration. Host preserves JS property/error/iterator
semantics, pure object construction, resolved callable/receiver identity and
library primitive semantics. Flattenability includes arrays, arguments and
spreadable values, with holes represented as undefined; the measured fixtures
cover dense values only. Inherited stats writes are delegated to the compatible
Host rather than silently reusing the older core's declared prototype boundary.

## Measured clean gates

At the code SHA:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --all-targets --jobs 2`: **0**, finished dev profile, zero errors.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --test legacy_socket_control --jobs 2`: **0**, **7 passed / 0 failed**.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **0**, output `Would update 0 manifests; 0 added, 0 orphaned symbols`.

The writer `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js
parity:extract` ran before the code commit on the same manifest inputs: **0**,
output `Updated 0 manifests; 0 added, 0 orphaned symbols`. No rust_files array
was hand-written. An earlier development compile failed (101) for unused test
import/field; both were removed. Development gates are not the SHA-bound gates.

## Native values and independent source floor

The oracle extracts the exact four members from each actual socket class and
loads actual endpoint declarations and jsonParse implementation. It uses the
installed lodash/flatten, not Array.flat, and independent Node HMAC-SHA256.
Native HMAC signature expectations are fixed from Python's independent standard
hmac/hashlib computation for secret `secret` and payload `|1700000000123`.

Rust independently pins **48 cases**, their family, iteration and fixture inputs.
Connect has 8 cases (stopped at 7), public open 4 (3), private open 8 (7),
subscribe 4 (3), public message 13 (12), private message 11 (10). Global source
enumeration stopped at **47**; this is a finite floor, not exhaustive socket,
coercion, callback, prototype, transport or signer coverage.

Before wire comparison, native assertions inspect UTF16 URL units, socket
assignment, listener identities/order, native constructed event/topic/auth
fields, f64 timestamp bits, fixed local and supplied remote signatures, signer
request fields, error values, effect order, route stats, live callback iteration
and captured send generation. Frame assertions retain the native object passed
to stringify BEFORE encoding; they do not parse encoded frames back into a
native value. Wire formatting is compared only after native assertions.

Public controls override stray data topics. Private receive diagnostics occur
before parsing; auth errors retain uncoerced message/error values and the exact
fallback. Signer availability errors retain `Remote WS signing unavailable:`
and do not send auth or fetch a snapshot. Remote rejection propagates. A source
key getter replaces sendWithLog while auth arguments are evaluated; native
callee capture retains generation 0 despite the getter advancing generation 1.

## Review finding

Read-only sub-agent review found the initial oracle's Array.flat substitution
was not lodash flatten, and it was replaced with the actual installed dependency.
No exhaustive sparse/arguments/spreadable fixture claim is made.

**Confirmed survivor:** removing only the private-open case 7 key getter
originally produced byte-identical 48-row source output (reviewer Node exit 0).
The index-derived keySwap label did not pin the fixture. The corrected input
observes the actual accessor descriptor; the getter's actual swap count is also
exported. Rust independently requires the accessor and generation advance.
Reviewer control exited 0 and observed true->false plus swaps 1->0; runtime
corpus discrimination is recorded below. No remaining material production
blocker was reported. Review ran no Cargo and made no edits.

## Discrimination

A compiling seven-family batch at the code SHA returned **101**, **0 passed /
7 failed**. Each mutation reached a different named test's native assertion:

| Native mutation | Assertion |
| --- | --- |
| message listener -> Close | `SOCKET native listener identity/order 0` |
| omit bbos public-open frame | `SOCKET native frame count` in native_public_open_frames |
| auth frame timestamp +1 | `SOCKET native auth timestamp` |
| ping dispatch -> ticks handler | `SOCKET native public message effects 4` |
| private ping response pong -> ping | `SOCKET native event 0` |
| flatten keeps only first array item | `SOCKET native frame count` in native_public_subscription_frames |
| omit algoexecutionreportv2 subscription | `SOCKET native frame count` in native_private_subscription_frames |

No workspace compilation, full crate test suite, clippy (host or wasm),
credentialed socket/auth test, v3 test suite, full class wiring or CI rerun was
performed. These scoped gates must not be reported as those claims.

The seven-family batch restored byte-identically and the same full scoped target
returned **0**, **7/7 passed**. An isolated compiling late-send-capture mutation
(read key before resolving sendWithLog) returned **101** at
`SOCKET native captured send survives key getter replacement`, then restoration
returned **0**, 1/1 with 6 filtered, using the scoped cargo test command.

For corpus-only controls, the restored Cargo-built test executable
`sdk-rs/target/debug/deps/legacy_socket_control-e9684d4ffb7dbc76` was invoked
directly with the same TypeScript environment and a named test filter. No Rust
code/binary changed during these controls:

- Removing the actual source key getter: **101**, `SOCKET corpus CONTENT input 19`; restored **0**, 1/1, 6 filtered.
- Shortening emitted corpus to 47: **101**, `SOCKET corpus SIZE`; restored **0**, 1/1, 6 filtered.
- Homogeneous output of 48 copies of row 0: **101**, `SOCKET corpus CONTENT iteration 1`; restored **0**, 1/1, 6 filtered.

Distinct discrimination floor: **8 native and 3 corpus controls**, stopped at
control **10** (zero-based). All credited controls failed at runtime; no compiler
error was credited. No remaining survivor was observed in this finite floor.
Source and oracle restore byte-for-byte, with SHA256:

- src/legacy_socket_control.rs: `72ac53731110cc6a662fd1aaa35eb1a0edc0c9c50db2c4bd2f8561b64492ab59`
- tests/legacy_socket_control_oracle.cjs: `85d15b7dc49ca2c0cf14f9af1fb08d94d97d90d7bc6d119356fb27763dd365c3`

## Integration preservation

Merged integration `775a019fe06014b8b858b3f8f6cbdabac552abaf` down, producing
`48f838d94fcc19e8a71af21f5b66ec8155423f64`. No conflict occurred.
At that merge SHA, the requested code-SHA-to-HEAD crate src/tests diff returned
**0**. The expanded input diff also returned **0** for all eight reachable
internal Rust packages, root/venue Cargo.toml, actual TypeScript inputs and WOO
manifests. The **481 reachable Cargo.lock package records** compare equal.
The 15 observed loaded TypeScript/lodash module files (enumeration stopped at 14)
were re-resolved and their contents compare equal to their code-gate snapshot.
This is the observed module floor, not a complete Node/platform dependency claim.

The merged `parity:extract:check` invocation returned **0** and output
`Would update 0 manifests; 0 added, 0 orphaned symbols`. Tomllib independently
reasserted 101 owned rows and exactly eight pending->ported transitions, yielding
**30 ported / 20 divergent / 51 pending**. Cargo was not rerun at the merge SHA:
its measured exits remain attached to `f6eb508d3de31de0ed16e5b9c9d554bcdb942108`,
with the byte-preservation proof above. This evidence-only follow-up changes no
gate input.
