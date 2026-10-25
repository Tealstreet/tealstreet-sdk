# BreakoutProp options refusal: delete the unreachable venue arm

Plan and controls, 2026-10-24. Lane claude-55ao7t; one-site slice and ruling by
claude-ddi2il (after #2048, `12da3363cb9`). Reviewer: codex-hcmq3f.

## Finding

The `on_command` arm for FetchOptionChain, FetchOptionExpiries and
FetchAccountGreeks (`lib.rs` ~1290, `cx.fail(command, "Options are not enabled
on this account")`) is unreachable:
- **Engine gate.** The engine refuses these commands before command entry
  whenever `venue.capabilities().options` is false (`engine.rs` ~3932,
  `refuse_before_entry` with the same text).
- **BreakoutProp capability.** BreakoutProp declares no options capability
  (`Capabilities::BASE`, `options: false`).
- **Evidence.** With the arm routed through `local_context::complete`, a NaN
  entry still returned the engine's untyped text. The venue was never called.

## Change (ruling)

- **Arm deleted.** The dead arm and its legacy inventory fingerprint
  (`cab66fc0…`, lib.rs) are removed. This is a genuine prune of unreachable
  code.
- **Exhaustiveness.** The three variants fall through to the venue's existing
  unsupported-command arm. There is no new Read wrapper, and a comment records
  why they never arrive.
- **Behaviour.** Unchanged: the engine's pre-entry refusal still answers.

## Control (`venues/breakoutprop/tests/options_local_context.rs`)

`the_engine_refuses_options_before_the_venue_is_called` covers all three
commands at a finite and a NaN clock. Each gets one terminal with the native
text, no kind, no outcome and no I/O. The venue's unsupported arm would answer
"Unsupported BreakoutProp command", so the text proves the venue was not called.
There is no RED: no venue change can alter an engine-owned refusal.
