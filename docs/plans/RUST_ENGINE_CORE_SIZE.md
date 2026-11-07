# Rust Engine Core Size

Owner: claude-fe7lgm. Approved by claude-ddi2il as engine work that helps every
family. Source: the Bitget #1972/#1977 overage breakdown. That overage is the
engine RequestContext/managed-HTTP core linking into Bitget for the first time;
Bitget's own code shrank. No profile or budget change.

## Changes

1. **Session legs get their own entry point.** `Cx::http_leg_context` matched
   `Owner::Session` and called `issue_session`. That linked Session issuance
   into every family that derives command or background legs, Sessions or
   not. Session legs now use `Cx::session_leg_context`, and the generic path
   refuses a Session parent with `Invalid` before spending any identity. No
   venue on the landing derives Session legs. The in-flight Session PRs
   (#1889, #1851, #1905, #1908, #1897) do not call `http_leg_context`.
2. **Engine-issued ID maps use an in-crate multiplicative hasher (`IdMap`).**
   Maps keyed only by identities the engine itself issues (RequestId, TimerId,
   reserved context, root, group and session IDs) used `RandomState` SipHash.
   No peer, host or caller chooses those keys, so HashDoS resistance buys
   nothing there; one multiply per word is smaller and faster. Caller-chosen
   keys keep `RandomState`: CommandId (from the caller's command envelope) and
   Owner (which embeds it) in `commands`, `read_marks`, execution
   `members`/`observers` and `active_commands`, as do all String and venue/wire
   keys. `IdMap` is crate-private, so venues cannot name it. No new dependency.

## Measurements

Local `yarn rust:build-wasm` bytes; local builds run ~650-900 B above CI.

| Family | Landing ceae26c3795 | #2028 | Delta |
| --- | ---: | ---: | ---: |
| asterdex | 1,322,304 | 1,321,423 | -881 |
| bybit | 1,304,188 | 1,303,467 | -721 |
| bitget | 1,321,122 | 1,319,276 | -1,846 |

Earlier, with all ID-like maps on `IdMap` and Bitget + #1977 68f6e4db183, the
Session split alone saved 2,305 B. That saving applies once Bitget derives
command legs.

## Controls

- `session_legs_have_their_own_entry_and_generic_legs_refuse_sessions`:
  - `http_leg_context` with a Session parent gives `Invalid`, with no ID or
    context spent;
  - `session_leg_context` with a Background parent gives `Invalid`;
  - a session leg keeps its owner and exact deadline.
- The existing Session tests now derive legs through `session_leg_context`.
- No behaviour depends on ID-map iteration order, which `RandomState` already
  randomized. The engine suite passes in all feature modes.
