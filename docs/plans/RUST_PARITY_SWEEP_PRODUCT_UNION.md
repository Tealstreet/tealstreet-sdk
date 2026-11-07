# Product-semantics union verification checkpoint

Frozen runtime: `8f71d5c4d3024c74d8ead0896ec56ec4cf26776f`. Subsequent documentation-only checkpoints preserve these runtime bytes.

This records completed primary gates and quality after merging landing PR #1341 and repairing the Phemex REST/WS total inconsistency. Full serial venue completion, final aggregate review and exact-head CI are recorded in [PR #1340](https://github.com/Tealstreet/tealstreet-sdk/pull/1340). Older `RUST_PARITY_SWEEP_FINAL.md` tables prove the earlier `96a39` runtime only.

- Locked workspace **2,684 native tests +3 doctests**, isolated feature-off engine **79 tests**, native/feature-off/WASM Clippy, formatting and bans PASS. Rust source SHA256: `34106ce98aa6e37f252fd66fe3e80f202da486653afdf89c37d1bdbe7b1d0e16`.
- Root typecheck/lint/test **44/28/27 tasks** PASS; SDK **7,670 tests across 520 files per target**. Source inventory unchanged through verification.
- Rebuilt combined engine **421 compiled SDK tests /12 files**, **68 host tests**, **16 product controls**, and combined GRVT private PASS.
- All four OKX private profiles and all four public captures PASS. Public output remains strict; no financial normalization/scoring relaxation.
- Selected OKX node/web each **824,320 bytes**, SHA256 `6109ce0aa7e5077a673eba855ccc69af1a2d733c4bbd305296b757ed07cdfb22`, below unchanged 1,258,291-byte ceiling.
- Combined engine (not a selected-venue budget artifact) **11,732,506 bytes** per target, SHA256 `e09563111217316847f13819b019b5a12089ce110530d1e020a64f07c00adc3d`.

| OKX public capture | TS / Rust main CPU ms | TS / Rust steady CPU ms | TS / Rust retained MB |
| --- | ---: | ---: | ---: |
| okx-linear-public | 1139.1 / 1001.6 | 862.9 / 743.3 | 19.2 / 5.3 |
| okx-inverse-public | 1966.9 / 1742.3 | 1446.3 / 1283.4 | 19.4 / 5.4 |
| okx-eea-public | 1697.5 / 1512.6 | 1296.8 / 1150.1 | 22.3 / 7.2 |
| okx-eea-incoming-public | 2203.3 / 1987.8 | 1668.8 / 1507.3 | 21.7 / 6.7 |

The remaining serial manifest requires136 steps:64 child replays (30 public/34 private),28 profile builds/25 unique child features,19 venue controls and11 integrations, plus574 shared harness cases or the updated current harness count. Parent OKX adds8 replays, yielding72 selected venue gates plus the separate combined GRVT proof; selected budgets cover26 unique features. CI inventory is102 replays (39 public/63 private),29 venue jobs +5 quality jobs. The fixed ceiling is not raised.

Independent union reviews preserve primary lookup/read routing, exact native fills and inverse quote notional, raw ledger currencies, USD-facing risk and real REST freshness provenance. The reviewed order-reserve currency fallback uses proven settlement metadata. Paired Phemex normalization preserves reported free balance, income, Triggered finality, failed-read atomicity and revision fences. Positive/negative two-currency regressions and both original failing cases are retained.

Serial scoring starts only after quality and primary gates finish. One process owns generated bindings; each step checks source and artifact identity and stops at its first failure. Any further diagnostic control requires an independently reviewed bounded plan and retains its first failure; no open-ended retry loop or score selection. Final quantities must come from actual receipts, not this inventory.
