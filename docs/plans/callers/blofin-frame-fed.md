# BloFin frame-fed mode

**Change.** BloFin is in `FRAME_FED_VENUES`.
- **At start:** a frame-fed BloFin engine reads only the public instruments. It skips the ticker seed, the private start (margin and position-mode reads, the 10 s poll, the private socket), and every account or order re-read (`refresh_account`, `refresh_orders`).
- **VENUE_STATE (version 1):** the account margin mode and its loaded bit, the per-order `Source` records, and the native fill progress. The data source exports it; the frame-fed engine applies it. An unreadable version leaves the applied state and logs `stream.malformed`.
- **Acknowledged ids:** pruned against the store at each write, so ingested ORDERS clear them.
- **Kept:** the amend's verify read (GET order-detail after its cancel) remains part of the write's own sequence, as in LOCAL (ddi).

**Callers.**
- SDK: none beyond the registry list.
- NX: none.

**Obligation.** None.

**Deletion prerequisites.** The forwarding write path for BloFin may go once the main-thread engine uses frame-fed mode.

## Settings holds

**Change.** A frame-fed BloFin engine holds its confirmed settings writes
(`safe_cex_engine::setting_holds`) until the source re-reads them:
- position mode under `Hedged`;
- leverage under `Leverage(symbol)`;
- the account margin mode under `Venue("margin")`.

The margin mode lives in VENUE_STATE:
- while it is held, `apply_venue_state` keeps this engine's value until the source reports the same one, which releases the hold;
- `on_frame_ingested` re-applies the held mode to flat symbols' settings rows, matching the write's own rule.

On the source, `refresh_settings` re-runs the existing margin-mode and position-mode reads and the account refresh, which carries leverage. It never writes.
