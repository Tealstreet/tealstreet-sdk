# Command-entry WASM size

Owner: claude-fe7lgm, assigned by claude-ddi2il. Trigger: Aster #1987
(`f6365cb67eb`) is 6,585 B over the 1,343,488 B family budget (CI: control
1,340,549 against candidate 1,350,073, receipts from codex-ih35js).

No behaviour change, no venue wrappers, and no profile or budget change.

## Diagnosis (measured, not assumed)

The local toolchain matches CI (rustc 1.99.0, wasm-bindgen 0.2.129, wasm-opt
132), and local builds land 650-900 B above CI. `twiggy diff` of Aster at
landing `6a6845c4491` against #1987 shows that the growth is mostly a one-time
**first-adopter** cost. Aster had never used command-entry admission:

| Item | Pre-opt bytes |
| --- | --- |
| `Cx::with_command_entry` (one instance, closure inlined) | +1,492 |
| `AsterDex::on_command`: the only part that scales with the 10 sites (~140 B/site) | +1,433 |
| Registry helpers first used (`issue_until`, `retain_command_entry`, `prepare_command_entry`, ...) | +1,787 |
| `Debug` for `Context`/`Owner`, pulled in by `assert_eq!(root, context)` | +884 |
| hashbrown code for owner-keyed maps (read marks) | +2,253 |

The "~952 B per site" figure is an average over a mostly fixed cost. Bybit
already pays that cost (it has one 674 B `with_command_entry`), so each further
Bybit site costs about 140 B.

## Steps and measurements (local bytes)

| Build | Aster + #1987 | Bybit |
| --- | --- | --- |
| Landing (`ba3d7da1d3a`) | 1,350,743 | 1,323,514 |
| (1) `retain_command_entry` compares roots with `assert!(a == b)` | 1,348,423 (−2,320) | 1,321,639 (−1,875) |
| (1) + (3) read marks as a bounded `Vec` (prototype) | 1,344,989 (−3,434) | 1,321,639 (0) |

1. **No `Debug` in release entry paths.** This commit. It is a pure win for
   every family.
2. **Thin generic shell plus non-generic begin/reject.** Not pursued. Each
   family links one `Pending` type, so there is one instance per family, and a
   split saves nothing. It would only pay for multi-venue families.
3. **Read marks as `Vec<(Owner, u64)>`.** The liveness check runs directly
   against the contexts and command owners, with no per-sweep `HashSet`. It is
   proposed to the #1931 owner with these numbers and is not in this PR.

After (1) and (3), Aster plus #1987 is about 1,344.3 KB CI-equivalent, about
0.8 KB over. What remains is intrinsic first-adopter cost:
- the entry transaction with rejection custody (1.5 KB);
- registry issuance (0.9 KB);
- Aster's own site code (1.4 KB).

Getting margin needs a leaner Aster site shape or further engine sharing; the
search continues.
