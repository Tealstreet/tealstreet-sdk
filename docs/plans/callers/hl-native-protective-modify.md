# Hyperliquid protective amendment safety

SDK, NX and hosted callers retain `AmendOrder`, its original five-second
pre-cancel bound and the returned replacement identity. All SL/TP trigger edits
use cancel/place; Hyperliquid requires non-trigger ALO/non-executable GTC for
modify without always_place, while always_place can place after failed cancel.
Neither trigger modify nor always_place is emitted.

The owner-bound preflight verifies identity, source sizing and native execution
parameters. The final cancel claim checks the symbol's fresh mark, current fills
and live closing-side position. A failed check is NotSent, preserving the old
order. Once cancellation has left, its pre-cancel deadline never abandons the
replacement. The captured builder decision needs no read between physical legs.
Existing 334 ms pacing and genuine nonce custody remain in force.

After definitive replacement refusal, only proven full-position isPositionTpsl
sources permit a one-shot original-parameter restore. Partial/attached edits stay
available; where grouping cannot be proven, no restore is invented. Their
failure carries Protection::Unprotected with “stop removed, please re-place”.
Unknown replacement outcomes must reconcile and must never automatically restore
or resend. Render Restored only after accepted recovery, or Unprotected after
failed/unknown recovery. A closed position, late source fill or newer protection
retires an unsent restore. Its final claim also checks the original fresh trigger.

Complete native stream fields outrank cached placement data. ACKs reconcile
accepted parameters without reversing newer fills/status, including stream-first
and ACK-first replacement orderings. Sibling protection remains independent.
No engine API, host policy, routing or public subscription change is required.
There is no caller migration or deletion prerequisite.
