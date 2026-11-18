# Binance spot and portfolio margin frame-fed

**Change.**
- **VENUE_STATE v2:** Binance's VENUE_STATE moves to v2 and still reads v1. It carries the stop-limit ids plus the PM order owners and the strategy and spot client-to-native maps, which PM and spot cancel routing need.
- **Pruning:** the source exports only rows for orders its store holds open, through the new `Venue::export_venue_state_for(&self, store)`. Its default calls `export_venue_state`, so other venues are unchanged. The source keeps its full maps, which also guard against reused client ids, so the section stays bounded and unchanged state is not resent.
- **Apply:** on the frame-fed side, the source's rows overwrite ours, and this engine's own fresh placements stay.
- **Frame-fed list:** binancespot and binancepm are NOT yet in `FRAME_FED_VENUES`. fe7lgm lists all four Binance variants with the settings holds (ddi).

**Callers.**
- SDK: none.
- NX: none.

**Wasm.** The binance cap rises to 1,510,400 B (1475 KiB): it measured 1,505,790 B locally, +4,753 B over the 1,501,037 B master; ceil KiB + 2 KiB, per the frame-fed ruling.

**Obligation.** fe7lgm's holds PR re-lists binancespot and binancepm.

**Deletion prerequisites.** None added.
