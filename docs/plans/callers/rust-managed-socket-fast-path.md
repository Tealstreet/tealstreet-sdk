# Managed-socket fast path for control-free feeds

A text frame on a current `ControlPolicy::None` feed is now admitted once,
not twice. Its traffic and receive bookkeeping run straight from that
admission, and its venue delivery skips the second policy lookup and the
redundant live checks. Validation keeps every such feed on `AuthMode::None` and
`TrafficPolicy::AnyText`, so the only deadline its message admission can meet
is idle expiry, and that is decided once at the frame's own `now`. Native-control
feeds, binary frames, opens, closes, timers and gated sends keep their existing
path.

## Obligations

- None for venues, NX or server callers. Delivered frames, retirements, timers
  and health are unchanged. The engine originals in
  `sdk-rs/engine/src/socket_message_ingress_tests.rs` and the BingX and WOO
  idle-deadline tests pin this behaviour.
- A future change that lets a control-free feed declare auth, a non-any-text
  traffic policy or a message-time deadline must route those frames back
  through `observe_control`, which admits them again.
