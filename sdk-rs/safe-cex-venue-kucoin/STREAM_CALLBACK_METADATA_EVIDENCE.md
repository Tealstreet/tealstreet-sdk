# Kucoin stream callback metadata evidence

Branch: `fix/kucoin-stream-callback-metadata`.
Actual integration parent: `f853d56fd227c2106dba609141b1fdd44bcab125`.
Reviewed, unmerged feature ancestry ends at futures private class
`6b59555c430a0a8d7cb0b3ff80c8b6c68d51abf1`, including the Spot private
class and account-response repair identified in that slice's receipt.
The integration merge before push reported already up to date.

At pushed gate SHA `82e82faf3177e89a3176ad878be21e288d56ca37`, the
`StreamOps.callback` factory receives source callable name and arity beside its
native body. The creation-site floor covers all ten sites in `public_streams`,
stopping at futures order-book cleanup: four parser/1 arrows, four anonymous
cleanup/0 arrows and two anonymous timer/0 arrows. Native fixture values store
those properties and expose them through Get. Assertions inspect the actual
returned cleanup, installed parser and timer handles before source comparison.
The maintained-source oracle separately inspects function properties. Normal
listeners cover the four parser/cleanup pairs; disconnected futures listeners
cover both timers. This does not certify arbitrary descriptors or an engine.

| Gate measured at that SHA | Exit | Result |
| --- | ---: | --- |
| scoped Kucoin cargo check, all targets, jobs2 | 0 | Zero errors |
| exact public_streams target | 0 | One test;61-case floor ends spot-book/iterable |
| exact spot_private_class target | 0 | One test, existing four native groups |
| exact futures_private_class target | 0 | One test, existing three native groups |
| parity:extract:check | 0 | Output: zero changed/added/orphaned manifests |

Three isolated production controls compiled and reached a named native
assertion. Each was restored byte-for-byte before the next control; each exact
public_streams target then passed at exit0:

| Control | Mutant exit | Named assertion | Restored exit |
| --- | ---: | --- | ---: |
| First futures candle parser name changed | 101 | STREAM native callback name | 0 |
| First futures candle cleanup arity changed | 101 | STREAM native callback arity | 0 |
| Futures candle retry timer arity changed | 101 | STREAM native callback arity | 0 |

No compiler rejection is counted as a semantic control and no survivor was
observed in these three iterations. This control floor stops at timer arity;
it is not an exhaustive mutation census of all ten call sites.
Read-only sub-agent review found no actionable issues and ran diff --check0;
it ran no Cargo or controls. The findings and fixes are posted on PR7922.

No manifest rows move. Independent tomllib census across12 Kucoin manifests,
stopping at kucoinspot.ws-public.ts.toml:148 ported /43 divergent /0 pending.
The public class assembly findings remain open. Relevant canonical scope is
`sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy`; no declaration is copied.

Not run: full venue/workspace/monorepo suites, host/wasm clippy, wasm build,
live transport, arbitrary-JavaScript engine conformance, or CI reruns.
