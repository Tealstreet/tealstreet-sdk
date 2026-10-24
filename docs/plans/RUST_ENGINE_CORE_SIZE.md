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
2. **ID-keyed maps use an in-crate multiplicative hasher (`IdMap`).** Every
   engine map keyed by an engine-issued integer identity (RequestId, TimerId,
   CommandId, u64/u32 context IDs, Owner) used `RandomState` SipHash. Those
   keys never come from a peer, so HashDoS resistance buys nothing there. One
   multiply per word is smaller and faster. String-keyed maps keep
   `RandomState`. No new dependency.

## Measurements

Local `yarn rust:build-wasm` bytes; local builds run ~650-900 B above CI.

| Family | Landing 2f5105596db | + Session legs | + IdMap |
| --- | ---: | ---: | ---: |
| asterdex | 1,322,304 | 1,322,304 | 1,321,100 (-1,204) |
| bybit | 1,303,973 | 1,303,328 | 1,303,082 (-891) |
| bitget | 1,321,122 | 1,321,122 | 1,318,980 (-2,142) |
| bitget + #1977 68f6e4db183 | 1,365,967 | 1,363,662 | 1,361,815 (-4,152) |

## Controls

- `session_legs_have_their_own_entry_and_generic_legs_refuse_sessions`:
  - `http_leg_context` with a Session parent gives `Invalid`, with no ID or
    context spent;
  - `session_leg_context` with a Background parent gives `Invalid`;
  - a session leg keeps its owner and exact deadline.
- The existing Session tests now derive legs through `session_leg_context`.
- No behaviour depends on ID-map iteration order, which `RandomState` already
  randomized. The engine suite passes in all feature modes.
