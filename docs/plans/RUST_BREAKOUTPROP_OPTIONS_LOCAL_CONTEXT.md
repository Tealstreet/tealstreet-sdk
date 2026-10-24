# BreakoutProp options local refusal custody

Plan and original RED, 2026-10-24. Lane claude-55ao7t; one-site slice ruled by
claude-ddi2il after #2048 (`12da3363cb9`). Base: `12da3363cb9`.

## Scope

One inventory site, `lib.rs` ~1293 in `on_command`. FetchOptionChain,
FetchOptionExpiries and FetchAccountGreeks answer
`cx.fail(command, "Options are not enabled on this account")` with no I/O.
#2048 moved the other local refusals in that `match` to
`local_context::complete`; this slice does the same for the options arm, and
nothing else.

## Contract

- The refusal consumes the genuine command entry as a Read through
  `local_context::complete(cx, Operation::Read, Err(..))`.
- **Valid entry:** the exact native text, no kind, no outcome, one terminal, no
  I/O, timer or Store change.
- **Invalid or expired entry clock:** the engine's typed entry refusal
  (`InvalidInput`), with no write outcome since it is a Read.
- **Unchanged:** no session, login, credential or key-selection change.
  Command routing is unchanged.

## REDs and controls (`venues/breakoutprop/tests/options_local_context.rs`)

- **RED.** `an_options_query_requires_a_valid_command_entry`: with a NaN entry
  clock, all three commands must fail typed `InvalidInput`. Today the native
  text is returned with no kind.
- **Control.** `an_options_query_keeps_its_native_refusal`: with a valid entry,
  the native text, no kind and no outcome, as one terminal with no I/O.

Reviewer: codex-hcmq3f, who owns #2048's local-custody helper and the adjacent
hunk.
