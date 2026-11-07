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


Plan review clarification: validate field types and closed enum values as well as
unknown fields. Probe a fresh instance for each command, because raw HTTP actions
do not carry a command owner and startup requests must never count as handler
proof. Classify legitimate precondition failures as ingress-only and empty or
deferred output as unproven, never positive dispatch. Native fixture data may be
venue-specific codec/state data but must not duplicate capability expectations.
The MEXC mutation proof must rebuild the isolated feature graph and retain normal
artifact proof; a combined artifact is not a substitute.


## First implementation slice

The existing family checks are routed through one shared catalogue/verifier.
Three original-red controls reproduce the old accepted constructor error,
returned unsupported failure and coded unsupported event; corrected focused tests
pass. Each actual command probe gets a fresh engine, and every alias/profile must
construct. Capability fields/types/enums are closed; policy-only fields are
classified rather than mapped to invented operations. Ingress evidence is
reported as precondition, terminal, request or deferred, never universal handler
proof. Combined artifacts use the same checker.

The native registry fixture selects REST-book claims from each engine's actual
record, seeds a valid market and requires exactly one outbound request without
transport dispatch. All eight current positive claims pass. This proves request
handler dispatch, not endpoint reply parsing or the venue's retry/completion
policy; those remain covered by the existing venue suites. No duplicate list of
expected venue capabilities is introduced. Other advertised commands' positive
valid-state handler coverage remains open.

The MEXC missing-feature control runs only in CI in a disposable detached tree.
It removes exactly the command-order-book opt-in, rebuilds the isolated MEXC
family, verifies the flag remains true and requires specifically the unknown
fetchOrderBook variant error. Build/constructor/entropy failures do not count.
The mutant never overwrites normal pkg artifacts or tracked manifests.


Independent adversarial review found that op-only validation could label malformed
HTTP/sign actions as request evidence and admit duplicate or mistyped terminal
output. The verifier now checks the required wire fields, completion ownership,
exclusive string result/error and at most one terminal. Added controls also keep
unsupported input-symbol errors as preconditions rather than inventing a missing
command. All 16 focused controls and all 1,130 harness unit tests pass; the
native eight-book dispatch fixture and strict registry Clippy pass. Release
artifact and isolated missing-feature mutation proof remain CI-only/pending.
Actual Aster profile #1734 `b95cad5a1003` is integrated; no own runtime source
changes were introduced by integration.


The final verifier controls also use the real engine fallback “This action is
not supported on this venue” and a named-method refusal. Both reject, while an
unsupported input symbol stays ingress-only. Error presence, not truthiness,
classifies terminal failures, including an empty message. The 17 focused
controls pass. Independent correctness review is clean; adversarial findings
were addressed with these bounded negative controls before publication.


A facade call-graph cross-check extends the catalogue for enum-backed commands:
`account: afterLoad` still sends FetchAccount after startup, and every cancel-all
mode sends CancelSymbolOrders (empty symbol only for native mode). These belong
in ingress checks even though their decoder variants are not feature-gated.
The additional regression selects these from the actual enum values; it adds no
venue expectation table or production change.


## Current landing integration

Actual #1756 `5f6488c7664` is integrated, including the Bybit dispatch signer,
all queued owned-history slices and the current shipped-family build policy.
The enum-backed account/cancel-all follow-up `525251fa548` is retained. Every
unowned tree entry equals this actual landing; the effective scope remains the
ten conformance workflow, harness, native-fixture and plan/index paths.

All 18 focused controls and all 1,132 harness tests pass after integration.
The native registry fixture passes against current construction/signing APIs.
Fresh CI must still prove all combined/standalone aliases and the isolated MEXC
feature mutant; current source-bound replay/package proof is required before
merge. Earlier CI and artifact evidence is historical. No production command,
capability or caller contract changes; other commands' valid-state positive
handler coverage remains an explicit follow-on.


Current adversarial review found that presence-only accountAuth validation could
accept malformed wire enums as deferred evidence. An original-red assertion
reproduces this; the shared verifier now applies the same closed credential,
scope, outcome and field contract as EngineHost. The corrected control covers
malformed values plus every valid combination. Production host behavior is
unchanged.


Fresh shipped-artifact CI caught a fixture precondition: TxFlow classifies
public-only trading as Unsupported before its existing cancel-symbol handler
can run. Keep the shared unsupported verifier strict. Its conformance fixture
now supplies an authenticated testnet setup with a valid scalar/account address
generated only in the short-lived test process, never persisted or logged. An
original-red/corrected-green fixture control pins these prerequisites. All other
venue setup profiles stay unchanged. An empty-order terminal is still ingress
evidence, not positive signed cancellation/request/reply proof. The failed run
39791929530 is retained as diagnostic evidence and cannot supply final proof;
corrected-head CI must rebuild all artifacts and repeat the MEXC mutant.


A focused sans-I/O check loaded the authenticated TxFlow CI package from the
failed run (ZIP digest, file hashes, checkout tree/parents and run/head verified).
The old public-only fixture reproduces the Unsupported failure; the corrected
ephemeral private fixture passes all five advertised probes. Empty-order cancel
completes without HTTP/WS/sign actions. This is one existing compiled package
check, not a local build, replay matrix or substitute for corrected-head CI.
