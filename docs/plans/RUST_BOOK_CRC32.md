# Shared order-book CRC32

## Scope and architecture

Start from actual landing `8e53b1bd39e0fd03534ffa4453349390251e4585`.
Bitget `src/book.rs` and Kraken `src/spot/book.rs` duplicate the reflected
IEEE CRC32 byte accumulator. Move that algorithm into one engine primitive;
keep checksum input serialization and book recovery in the venues.

The architecture gate answer is yes: this is a cross-cutting concern in two
venues, resolved by one shared implementation rather than another venue fix.
The socket/signing and history owners confirmed both book regions and an
additive engine export are clear. Open #1785–#1788 change account/trading
reporting or freshness, not these book files. Preserve all shared exports on
later integration. This PR waits behind #1789 and #1785–#1788.

## Plan

1. Commit this plan and its index row; obtain independent correctness and
   adversarial plan reviews before source edits.
2. Add tests through both actual venue engines using a proposed
   `safe_cex_engine::checksum::Crc32` byte-stream API. Record original compile
   RED for the missing shared API separately for each venue. Fixed CRC literals
   from an independent Python zlib calculation also test the native book
   snapshot/update/mismatch/recovery path. These are API-adoption REDs, not
   claims of a pre-existing wrong-checksum production bug.
3. Add `engine/src/checksum.rs` and one `lib.rs` export. `Crc32::new()` and
   `Default` start at all ones; `update(&[u8])` and `update_byte(u8)` advance a
   private state; `finish(self) -> u32` complements it. A compile-time 256-entry
   table implements reflected polynomial `0xedb88320`; no allocation, dynamic
   setup, dependencies, profile flags or platform-specific implementation.
   Test empty/standard/binary vectors, every split, bytewise versus chunks,
   and an independent test-only bitwise reference over every byte value.
4. Replace both local CRC loops with the shared API. Bitget still interleaves
   bid/ask top 25, separates original decimal strings with colons, and casts the
   unsigned result to signed i32. Kraken still consumes normalized decimal
   digits for asks then bids, top 10 each, without separators. Preserve omitted
   checksum behavior, level trimming, zero-quantity deletion, mismatch reset,
   resubscription, sequence handling and all original fixture bytes.
5. Run focused engine/Bitget/Kraken/Binance native tests, strict all-target
   Clippy/fmt/bans, root typecheck/lint and root tests. Obtain independent
   implementation/test reviews; record exact checks and RED/GREEN receipts.
   Update nearby audit/quality/caller docs describing landed behavior, then
   open a draft PR with architecture and queue boundaries explicit.
6. CI owns WASM sizes, all replay/timing gates and byte-identical nonmetric
   replay comparison (both venues/aliases and shared-engine full roster).
   Authenticate current baseline/candidate archives, physical jobs and build
   inputs; retain strict comparison and independently review evidence.
   Report the Bitget CPU ratios from this PR's CI. No local release/replay
   build, no build-profile/opt-level/CPU threshold/gate change and no performance
   claim from the table alone. Any gate failure follows the owner's decision;
   this work does not reopen the rejected Bitget speed-profile experiment.

## Risks and evidence boundaries

The initial/final XOR, signed high-bit result, stream chunk boundaries and
decimal spelling are observable checksum boundaries. Tests must cover both
successful checksums and a rejected wrong checksum, unequal sides, truncation,
deletions and trailing zeros. No floating-point reformatting is introduced.
The shared lookup table has a static 1 KiB footprint; CI must report optimized
family WASM deltas and headroom rather than assuming its final artifact cost.
The earlier Bitget diagnostic tape has no checksummed books, so its CPU ratio
does not establish CRC speedup. Native checksum-bearing controls supply that
correctness coverage; unchanged replay outputs establish compatibility only.

No guard or merge precedes actual #1789, #1785, #1786, #1787 and #1788. Real
integration/fresh proof requirements apply to all changed shared inputs.
