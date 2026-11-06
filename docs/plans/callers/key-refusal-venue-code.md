# Bitget key-refusal telemetry

Bitget private WS route-refusal and exhaustion `auth.rejected` notices now carry
their actual numeric 40018/40024 as string `fields.venueCode`, matching the
structured diagnostic path. Message text, levels, route selection and retries
are unchanged. Consumers use the field rather than extracting a bare number
from text. Worker RPC serialization already preserves string `venueCode` and
is unchanged; four round-trip controls pin that contract across venue codes.

The broader audited proposal is `docs/plans/RUST_KEY_REFUSAL_TELEMETRY.md`.
Other producer fixes belong in its second PR; no codes are manufactured for
local validation or HTTP-only failures. This PR is not merge authorization.
