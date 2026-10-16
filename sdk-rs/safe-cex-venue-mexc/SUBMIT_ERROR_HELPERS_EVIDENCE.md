# MEXC submit-error helpers

**2 pending→ported, 0 pending→divergent**. Python 3.14 tomllib census over six manifests, stopped after mexc.ws-public.ts.toml: **90/40/55**, 185 rows. Code/gate SHA `246e85a86097c79325519a54ed3be303b000d2c4`, direct integration parent `ed8336d5835189fd470040fbe62756ff1b60a0ae`. Branch `feat/mexc-submit-error-helpers-ed8336`. No independent unreviewed branch ancestry; the prior MEXC rescue is part of the requested integration parent. Subsequent commits contain tracked evidence only.

Ports `MexcExchange#isNativeBatchAccessDeniedError` and `MexcExchange#noteRegularOrderSubmitError` as actual free functions in exchange_helpers.rs. They retain nullish response/root field precedence, source toNumber/String error ordering, the three denial phrases, conditional clock access and source1500ms Math.max cooldown update. Cooldown state is supplied by the caller as the source field. The submission queue and placement methods remain pending; no request placement is claimed.

The oracle extracts/transpiles both original method bodies and original cooldown constant, using maintained toNumber. Independently expected floor: **33 error fixtures**, stopping at case32; **four cooldown states** per case, stopping at state3 (negative infinity/positive-infinity clock), **132 state transitions**. Native bool, coercion error name/message, f64 bits/NaN and clock calls are checked directly. No encoded/roundtrip comparison is used as proof. The floor covers represented JSON/native Value inputs, including absent/null fields, authoritative0/false/empty strings, parseFloat string/array values, message case, malformed coercion objects and unchanged/extended/NaN/infinite cooldowns; no exhaustive JS input claim.

At the exact SHA above (raw logs/command arrays tracked in evidence/submit-error-helpers/):

| Gate | Exit | Scope |
| --- | --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-mexc --all-targets --locked --jobs2 | 0 | MEXC and dependencies |
| cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-mexc --test submit_errors --locked --jobs2 -- --nocapture | 0 | Both native oracle tests |
| cargo fmt --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-mexc -- --check | 0 | MEXC formatting |
| python3.14 sdk-rs/safe-cex-venue-mexc/scripts/scoped-parity.py | 0 |185rows,135public Rust items,zero venue exemptions |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0 | Read output:7341symbols,512files,zero stale/added/orphaned |
| python3.14 sdk-rs/safe-cex-venue-mexc/scripts/submit-error-controls.py | 0 | Two actual production controls |

Fixed two-control floor, stopped after submit-cooldown. `code == 511.0`→512.0 failed native assertion `MEXC native batch denial:` (101); baseline/restored each0. `now() + 1_500.0`→1_499.0 failed native assertion `MEXC submit cooldown:` (101); baseline/restored each0. No survivors; production file restored exactly. Oracle gates/control commands use `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

Sub-agent static review found no actionable mismatch for represented inputs and verified both real Rust targets; reviewer ran no gates. This slice did not rerun the full MEXC suite, full core suite, workspace/monorepo builds, host clippy, wasm check/clippy, CI or live exchange calls. The108-test rescue result is separately attached to its own SHA, not claimed as a run at this SHA. No Polymarket row movement is claimed in this slice.

## Merge-down and new measurements

Merged integration `f436fc816390a8b4556ab7a2fe0ae85072337036` at `64a436426e0f07b73d8b679f81b480c0684c97af`. The requested crate-src/test diff against246e exited1: incoming utils.rs added `is_perpetual: None` to market normalization. Both new helper functions, tests/oracle, controls and mappings were byte-unchanged. Shared Market/output/patch dependencies changed. Therefore the earlier code-SHA exits were not reported as fresh measurements at the merge.

Re-executed all six gates at64a436: **scoped all-targets check0; native helper plus existing utility tests0 (2+6 tests); formatting0; official MEXC checker0 (185rows,135public items,zero exemptions); extractor0 (7341symbols/512files/zero stale/added/orphaned); production control script0 (both baseline0/mutation101named/restored0)**. Exact command arrays and raw logs/controls for this merge SHA are tracked in evidence/submit-error-helpers/post-merge/. Census remains90/40/55 with exactlytwo pending→ported relative toed8336 and refreshed integration. No full MEXC suite run is claimed at64a436.
