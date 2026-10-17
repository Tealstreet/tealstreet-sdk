# Parity default formatter repair

Integration job [116323440753](https://github.com/Tealstreet/tealstreet-next/actions/runs/38705397284/job/116323440753) at `d74a1c01f77439f9df61fa330063be194aab7576` passed the repaired Apex step and failed the next Binance generator: default stable had no `rustfmt`. The types job installs stable with a minimal profile in an isolated toolchain home. The Binance generator runs at repository root, so the workspace pin does not select its formatter.

The types bootstrap now explicitly installs `rustfmt` on default stable. The generator retains its nonzero-formatter assertion.

`parity-root-rustfmt.json` binds the seven passing standalone checks to the tested commit. The same checker fails on the parent workflow and on deliberate deletion of the repaired component, then passes after restoration. Additional controls reject clippy-only setup, setup in the native job alone, late setup, conditional setup and ignored bootstrap errors. This is a lexical workflow contract; successful component installation and the full parity job remain CI claims requiring an actual successful job.

The existing mandatory CI-wiring suite imports the standalone check. Its Cargo-dependent full invocation was not run concurrently with active Coinbase tests. No parity rows changed. Sub-agent static review found no material blocker in the source fix; the review does not claim runtime execution.

