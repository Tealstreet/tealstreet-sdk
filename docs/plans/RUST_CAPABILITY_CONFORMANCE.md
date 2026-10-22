# Capability and compiled-feature conformance

## Goal and boundary

Owner scope after the background account reporter: CI must catch an advertised
command missing from a shipped standalone family, including the MEXC #1681 class.
It must also distinguish decoder admission from a working handler. No runtime
policy, signer migration, venue-name fallback or new command is authorized here.

Start from actual provider #1731 `06b321192a6`; #1735 background reporter is under
review/CI and will be integrated before final proof as required. Coordinate
workflow/family harness overlap with the browser/profile lane before editing.

## Existing gap

`harness/families.integration.mjs` already loads the actual isolated artifacts
built with `--no-default-features --features <family>`, but probes only REST book
and schedule-cancel ingress. It accepts same-family construction errors unless
they contain “unsupported venue”, and `doesNotThrow` discards returned actions.
Thus a skipped constructor or returned unsupported failure can pass. Workspace
native tests unify Cargo features and cannot prove standalone decoder coverage.

## Committable phases

1. Add one shared capability-to-command catalogue and strict action verifier in
   the harness, with unit negative controls. Select commands from the actual
   capability record, explicitly classify policy-only fields and reject unknown
   record fields. Canonical fixtures cover each advertised optional command and
   feature-gated variant; do not maintain per-venue capability expectations.
2. Wire the catalogue into the actual family suite. Require successful coverage
   for each alias/profile using valid construction options; any unclassified
   construction failure fails. Send JSON through WasmEngine.command, inspect
   correlated completion/event/request actions and reject unsupported dispatch.
   Keep outbound requests sans-I/O. Exercise combined artifacts with the same
   checker so combined-only feature activation cannot hide a standalone gap.
3. Add positive dispatch coverage with valid native fixture state, starting with
   the shared REST-book command across its advertised venues. Distinguish this
   evidence from ingress/precondition coverage: unknown market, no credentials
   or not-started refusal is not positive handler proof. Extend valid fixtures
   for other advertised commands in bounded follow-ons; report uncovered handler
   cases explicitly instead of calling parser acceptance universal conformance.

## Verification and architecture gate

Original-red controls must catch the old silently-skipped constructor and ignored
unsupported action. Further controls cover malformed/missing action output,
uncorrelated completion, policy-only fields and newly unclassified capabilities.
Removing MEXC's command-order-book dependency must fail its isolated artifact
probe despite the advertised flag; the control may run in CI because local WASM
release builds are prohibited. A synthetic advertised-but-unsupported handler
must fail, while a precondition-only probe must never count as positive dispatch.

Run focused harness unit checks via explicit Node24 PATH and corepack yarn only,
strict native tests/Clippy for any native fixture target, and full CI. No local
release WASM, replay sweep, size or timing matrix. Independent plan/correctness/
adversarial review must answer: this is one cross-family CI contract checker,
not another per-venue cross-cutting implementation. Preserve current capability
values, production API/default loaders and caller behavior. Document each phase's
actual proof limits and keep remaining handler coverage open until exercised.
