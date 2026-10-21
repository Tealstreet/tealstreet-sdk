# BingX accepted IDs on batch failures

The shared port-review batch finding remains real: a successful earlier step
adds its acknowledged native IDs to `Job.ids`, but final failure drops them.

1. Reproduce native rejection, transport loss and malformed acknowledgement
   after earlier successful placements through the existing trading harness.
   Check native diagnostics, one completion, unsent tail and existing outcomes.
2. Attach the known accepted IDs with the existing `VenueError::partial_ids`
   helper at final completion. Preserve existing side-effect/outcome tracking,
   request order, stop/continue policy and original failure classification.
3. Update nearby venue/audit documentation, obtain an independent review, and
   run focused BingX/Binance tests, strict Clippy, formatting and bans. Push for
   full CI, guard separately against current landing and merge on green.

The existing typed/legacy partial-error shape is reused; no new caller API.
Callers can reconcile acknowledged native IDs before deciding whether to retry.
