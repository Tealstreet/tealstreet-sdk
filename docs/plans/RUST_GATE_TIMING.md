# Landing timing gates

Branch `fix/rust-gate-timing` from `feat/rust-rewrite` at `d17518bc5f`.
Landing was red on two public CPU objectives: Hyro steady CPU (runs
`39433770908`, `39438422053`, `39441654489`) and Paradex main-thread CPU
(`39429987138`, `39438422053`).

## Evidence (35 CI runs of the bybit and paradex gate jobs, 2026-10-20)

| Objective | Rust/TS ratio median | range | note |
|---|---:|---:|---|
| Hyro steady | 0.97 | 0.82–1.19 | TS 2.5–7.8 ms; Rust−TS within −0.7…+0.9 ms |
| Paradex main | 0.87 | 0.79–1.05 | ≥0.96 only on fast runners (TS ≈ 290–350 ms) |
| Paradex steady | 0.64 | 0.59–0.86 | always passes |

- Hyro: a synthetic 160-frame tape. Its steady window is 120 frames (60 tickers,
  60 small book deltas) and about 5 ms total; earlier lanes' steady-window
  profiles found no wire hotspot (RUST_AUDITS.md). Both arms are tied, so a
  strict `<` on this window passes or fails by chance. Every other public
  steady window is at least 10 ms TS.
- Paradex: startup streams 19.5 MB (`/markets` 9,322 rows, 9,254 options;
  `/markets/summary` 9,323 rows). Each row was scanned three times: the
  splitter's byte state machine, an `IgnoredAny` validation pass, then the
  venue's identity parse. Natively these cost 28/17/17 ms for markets and
  10/7/8.5 ms for the summary.

## Plan

1. Engine: scan array elements by spans, stopping only at bytes that change
   state. Paradex opts into `ItemCheck::Venue`: its identity parse
   deserializes every row with serde_json, so the splitter's syntax pass only
   repeats it. Regression: malformed rows, including filtered option rows,
   still reject both seeds. Native markets+summary streaming 62 → 31 ms.
2. Gate: objectives whose TS median is under 10 ms compare with a 1 ms
   absolute tolerance (the largest Rust−TS gap observed on a tie was 0.9 ms).
   Larger objectives keep strict `<`. Prove with a deliberate Bybit slowdown
   control on CI that the gate still fails Hyro and the Bybit variants.
3. Adversarial review, PR into `feat/rust-rewrite`, merge on green CI.
4. Review PR #1344 (batched delivery, isolated venue WASM packages): land clear
   wins with evidence, close the rest with a written reason.

## Small windows: nine pairs and a floor (after #1456, with #1346)

After #1456 recovered the Hyperliquid Spot cost #1356 added, its CPU
objectives (TS main 40-70 ms, steady 10-18 ms over 50 book frames) still swung
between runners: steady 0.73-1.08 and main 0.81-0.995 Rust/TS over five CI
runs of unchanged code, failing on fast runners (run 39485779122: steady 12.2
vs 11.3 ms). #1346's policy (seven interleaved pairs, Rust at least 10% below
TS) would make such windows a permanent declared failure. With the overseer's
approval the single policy in `sdk-rs/harness/performance-policy.mjs` now gives
main/steady objectives under 100 ms TS nine pairs and passes them when Rust is
at most max(1 ms, 5%) above TS; 100 ms and above keep the 10% rule, and process
CPU and memory keep it at every size. This subsumes the sub-10 ms 1 ms
allowance above. It cannot catch a regression that leaves Rust within 5% of TS
on a small window (#1356 on Hyperliquid Spot was one); a per-message slowdown
on Bybit and Hyperliquid still fails (evidence on PR #1461).


## Fixed balanced public budget (2026-11-04)

`fix/replay-cpu-counterbalance` replaces the timing-dependent seven-to-nine
extension with sixteen fixed pairs for every public case. Eight pairs run TS
first and eight run Rust first. The same budget applies to the independently
sealed remeasurement and each Bitget paired cohort; their validators require
all 34 original slots (one audited pair plus sixteen performance pairs).
Bitget retains its eight-cohort order, frozen calibration and tolerance; the
protocol digest records the new sample inventory. Private correctness counts
are unchanged.

Main/steady small-window allowances, process/memory objectives, independent
per-implementation medians and the strict 1.05 remeasurement eligibility bound
are unchanged. There is no stop-on-pass, sample removal, runner-class baseline
or tolerance adjustment. CPU model, guest topology and frequency observations
are retained outside the child CPU interval, with the physical sample order
and elapsed time. Missing topology/frequency is explicitly unavailable;
`os.cpus()` speed is labelled nominal. A sibling `measurement.json` also
survives an interrupted replay child. Offline verification does not invent
current-machine metadata for an old measurement.

The optional dispatch-only diagnostic uses identical source and one immutable
TxFlow module in both arms. Each original runner collects all sixteen pairs
per arm in a fixed ABBA/ABBA block order, with equal TS-first/Rust-first pairs.
Its predeclared twelve-pair prefix is diagnostic only, and every sixteen-pair
original remains in the artifact. Optional GC/tiering traces run afterward,
outside the timed cohorts. These artifacts grant no CPU certification.

Run `41874645362` captured Intel 8370C and AMD EPYC 7763, 9V74 and 9V45.
The twelve-pair prefix gave conflicting strict verdicts between identical-code
arms on EPYC 7763; sixteen pairs gave matching verdicts on all four runners.
Raw ratios still varied by 7.4% on 9V74, so matching verdicts do not establish
that all timing variance is below 5%. EPYC 7763 remained a genuine failure:
TS steady medians 18.3535/18.243 ms versus Rust 19.387/19.5505 ms.
The measurement change does not waive that failure.

Measured child wall costs were 65–73 s per twelve-pair case and 87–97 s per
sixteen-pair case. Relative to nine pairs on these same runners, sixteen adds
about 39–42 s per case, or 28–30 aggregate runner-minutes over the 43 public
cases before any eligible second measurements. The overseer approved that
cost. Twenty-four pairs would cost another 44–48 s per case over sixteen.


Independent original run `41876207480` retained four more complete A/A
cohorts: 8370C, 9V45 and two 9V74 runners. Every sixteen-pair arm passed the
unchanged strict objectives, with matching A/B verdicts. Together the two
runs cover eight runners and 512 original performance children: sixteen-pair
verdicts agree on all eight, while twelve pairs disagree on the first run's
7763. The second 9V45's raw contrast was still 6.1% apart. Sixteen is the
smallest evaluated budget with consistent strict verdicts across this evidence,
not a guarantee against future variance or an exemption for the 7763 failure.
