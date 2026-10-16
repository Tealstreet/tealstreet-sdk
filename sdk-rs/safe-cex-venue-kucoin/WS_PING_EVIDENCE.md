# Four pending ping methods ported

Moved **4 pending to ported, 0 pending to divergent**.
Tested code SHA: `0ab77727b93dbad72179c0f0bf8d519d2c843522`.
Fresh branch base: `682d9abac248136e286eee554677f922fd9e6672`, exactly the
requested integration head. No unreviewed feature branch was adopted as an ancestor.

Python 3.11 tomllib parsed all twelve KuCoin manifests and compared each symbol
by ts/kind. That integration base has 63 ported / 35 divergent / 93 pending;
the tested tree has **67 / 35 / 89**. The two metadata getters are in a separate
PR and are still pending in this particular base; they were not adopted here.
The only four changed statuses are the futures/Spot public/private ping methods.

Each method guards disposal before both clocks, updates the existing core
monotonic ping timestamp, reads the host Date.now callback and sends ordered
id/type heartbeat JSON. Public methods use send; private methods use send_with_log.
No venue heartbeat state, timer, string input or representation restriction was
introduced. Concrete socket classes, token acquisition and lifecycle methods
remain independently pending.

Completed gates bound to the tested code SHA:

| Gate | Exit / measured result |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2` | 0; zero compile errors |
| `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --jobs 2 --test ws_helpers` | 0; eight tests passed, zero failed/ignored |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; output inspected: 7341 symbols / 512 source files, zero changed manifests, added or orphaned symbols |
| `rustfmt --edition 2024 --check` on ws_ping.rs, lib.rs, ws_helpers.rs and support/ws_ping.rs | 0 |

Manifest mappings and provenance arrays were written through extractor
parseManifest/serializeManifest, followed by the full parity:extract writer:
exit 0, zero updates, added or orphaned symbols. No array was hand-spelled.
The new proof is a support module included by the existing websocket oracle
target. Package.json is byte-unchanged from the base, independently checked
with git diff --exit-code: exit 0.

One separate compiling production control per row changed only that method's
ping_frame(date_now()) to ping_frame(date_now() + 1.0). Each exact run of
`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --jobs 2
--test ws_helpers ping::four_live_pings_preserve_native_clock_identity_disposal_and_failure
-- --exact` exited **101** at its named native assertion:

- ping_futures_public: `KUCOIN-PING native id kucoin-public/0`.
- ping_futures_private: `KUCOIN-PING native id kucoin-private/0`.
- ping_spot_public: `KUCOIN-PING native id kucoinspot-public/0`.
- ping_spot_private: `KUCOIN-PING native id kucoinspot-private/0`.

Source was restored byte-for-byte in finally after each mutation. Every
restored exact test exited **0**. None of these four controls survived;
no other new mutation is claimed.

The new proof reaches twelve cases (four methods, three outcomes), stopping
at the fourth method / outcome 2. Independent expectations pin eight frames,
two decoded fields per frame and the performance/date/send effect sequence.
Decoded transport id/type, native error metadata and actual core pong latency
are asserted before any source wire comparison. Pong latency observes the
real shared ping timestamp; it is not claimed as direct access to that private field.
The live source captures pingAt bits separately. The corpus has finite clocks
and open transports plus disposal and send failure; it is not an exhaustive
clock or send-state census.

The oracle executes actual venue ping bodies with injected clocks and sends.
Rust uses the real BaseWebSocket and WsExchangeParent with a fixture native dial.
This proves the method slice, not production socket routing or full lifecycle.
Static sub-agent review confirmed method parity, native assertion ordering,
all four mappings/provenance and oracle wiring, with no findings. It ran no gates.

Initial check at `49a7934ec0` exited 101 for an unused fixture import; the
following commit fixes it. This is not counted as a pass. No CI was rerun.
Not run: complete venue suite, workspace build, host Clippy, wasm build/Clippy,
or Rust parity tests. The subsequent evidence-only commit changes documentation,
not the tested code, scripts, tests or manifests.
