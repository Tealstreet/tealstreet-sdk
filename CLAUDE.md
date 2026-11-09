# Claude Guidance

Read [`AGENTS.md`](AGENTS.md) first. It is the canonical root guidance for all
coding agents in this mirror.

Package- and app-specific `CLAUDE.md` files still apply. Before editing package
or app code, read the nearest `CLAUDE.md` and treat it as the more specific
context.

## Rust execution modes and write path

LOCAL and COSIGNED manual writes stay on the page engine and their existing
direct/proxy/tunnel routes. Their existing rules are unchanged.

For DELEGATED manual writes only, when `delegated.enabled` is true for the venue,
use the existing server `callMethod` path and the account's resident signer:
this is the only time sending orders through the server is allowed. Keep the
existing server order audit and never automatically resend an Unknown outcome.
