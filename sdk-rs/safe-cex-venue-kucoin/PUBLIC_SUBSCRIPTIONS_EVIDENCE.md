# KuCoin public ticker subscription parity

Three rows moved pending -> ported; zero divergent rows added. This slice moves
seven rows with OKX's four. Independent Python 3.11 tomllib census covered all
12 KuCoin manifests, stopping at `kucoinspot.ws-public.ts.toml`: parent 73 ported /
39 divergent / 79 pending; delivery code head 76 / 39 / 76. Census is not proof.

Fresh parent: `c54039795784efc67fbfa0bb7d0109e47440ee11`.
Branch: `feat/okx-kucoin-market-reads`; no inherited old feature branch.
Source/control SHA: `354cf116b3c1d6eae2a3d9a88df50272102ebdaf`.
First integration merge: `ebcb51ffa55881e2e5a745c0d419028b0632da7d`, incorporated
at measured gate SHA `4869a732862b7f2bda8a9462a6cc8bc0286d152d`.
Pre-push integration merge: `d4d9ae0b019e4c5466a67294aa474f897ee38061`, incorporated
at metadata SHA `1d0e7bd41a3f6b05b5210fb7980f41278822b9ed`.

`KucoinPublicWebsocket.subscribe`, `subscribeActiveTicker`, and
`unsubscribeActiveTicker` map to `subscribe_public`, `subscribe_active_ticker`,
and `unsubscribe_active_ticker` in `src/public_subscriptions.rs`. Snapshot-only
subscription has no disposal guard and invokes the channel operation even for
an empty shard. Active operations require a nonempty UTF16 symbol and a live
disposal flag, retain exact tickerV2/instrument channel order, and distinguish
subscribe from unsubscribe. The shard getter and channel operation are injected
dependencies; original errors propagate unchanged. Concrete websocket owning
class and core send/background reconnection wiring remain pending. This slice
does not resolve the previously noted non-OPEN core-send seam.

## Gates actually measured

Oracle environment:
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Commands ran sequentially, never with concurrent cargo invocations.

| Command | Measured SHA | Exit |
| --- | --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2` | `4869a732862b7f2bda8a9462a6cc8bc0286d152d` | 0 |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --jobs 2 --test public_subscriptions` | `4869a732862b7f2bda8a9462a6cc8bc0286d152d` | 0 |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | `4869a732862b7f2bda8a9462a6cc8bc0286d152d` | 0 |
| Same exact extractor check | `1d0e7bd41a3f6b05b5210fb7980f41278822b9ed` | 0 |

Extractor text reported zero updates, additions, and orphaned symbols. Writer
was run after manifest edits and reported the same. The explicit KuCoin venue
oracle now includes `--test public_subscriptions`; the whole venue command was
not run. The separate existing-binary Rust-index gate exited 1 at both measured
SHAs on a shared webworker thread_local macro before invention/status validation.
See `../safe-cex-venue-okx/REFERENCE_PRICE_EVIDENCE.md`; no full-index pass claimed.

The gate asserts all native operation/channel/symbol arguments, literal call
counts, lazy call order, full error values and Rc identity before the live source
oracle. The independent floor is 32 active cases (2 operations x 2 disposal
states x 4 UTF16 symbols x 2 outcomes) plus 18 snapshot cases (2 roles x 3 shard
fixtures x 3 failure sites). UTF16 includes empty, scalar, comma, and surrogate
fixtures. Snapshot fixtures include zero, three including a duplicate/surrogate,
and 101 symbols. Enumeration stopped at unsubscribe / disposed / surrogate /
send failure, and snapshot / 101-symbol shard / send failure. This is a finite
argument-level floor; no network frame or subscription acceptance claim follows.

## Compiling controls and byte preservation

At `354cf116b3c1d6eae2a3d9a88df50272102ebdaf`:

| Deliberate production mutation | Failed native assertion | Mutated / restored exits |
| --- | --- | --- |
| Snapshot role guard -> detail | `KUCOIN-PUBLIC native snapshot lazy call order` | 101 / 0 |
| Active subscribe -> unsubscribe | `KUCOIN-PUBLIC native active operation` | 101 / 0 |
| Active unsubscribe -> subscribe | `KUCOIN-PUBLIC native active operation` | 101 / 0 |

Each mutation compiled and each restore reran the identical focused test command.
Zero survivors among these three controls. Scoped source/test/script comparison
against the control SHA after the OKX-only review fix exited 0. First merge's
whole-venue comparison against `3823d4807c` exited 0. Final merge's src/tests/
scripts comparison against measured cargo SHA `4869a73286` exited 0; checked
shared Rust dependency paths had zero changes and TS oracle inputs compared
equal with exit 0. Cargo exits stay attached to the actual measured SHA. Metadata
changes only required the fresh extractor at `1d0e7bd41a`; exact rename notes
also corrected the preceding disposal rows without changing their status.
Receipt commit `5510394b7d` differed from `1d0e7bd41a` only in these two receipts.

Sub-agent static review reported no remaining blockers after the exact notes
and host-plumbing corrections; it did not run gates. Not run: workspace build,
full venue suite, host/wasm clippy, CI, live network frames or concrete class wiring.

## Final merge and metadata refresh

Final integration parent `9f59912c6f39a0df543e6228c38ad5a57258d2a2` was merged at
`10fddfe345549783937235fb8566a72524d055e6`. One exempt.toml append conflict was
resolved by retaining our five OKX entries and all 22 incoming Bitget entries;
serialized union validation and sub-agent parent-diff review confirmed every
entry and reason survived, with no duplicate or resolution finding.

Exact venue src/tests/scripts diff against `4869a73286` exited 0; checked shared
Rust dependencies, Cargo inputs, and live TS/oracle-loader inputs had zero changed
paths. The cargo check/test exits remain bound to the measured SHA `4869a73286`.
Fresh extractor at `10fddfe345549783937235fb8566a72524d055e6` exited 0 and reported
zero updates/additions/orphans. No final-merge full Rust-index gate was run; the
earlier shared-macro failures remain findings, not passing measurements.

Tomllib comparison at this SHA covered all 12 manifests per venue and measured
exactly three KuCoin pending -> ported changes (four OKX), zero other status
changes, and KuCoin 76 ported / 39 divergent / 76 pending. Delivery after this
merge changes only the two manual receipts.
