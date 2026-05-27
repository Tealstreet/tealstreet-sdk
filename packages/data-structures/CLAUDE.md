# CLAUDE.md — @tealstreet/data-structures

Custom data structures for efficient real-time data management.

- **Key structures**: `RingBuffer`, `Queue`, `QueueWithMaxSize`, `PriorityQueue`, `TimeLimitedQueue`, `RandomElementSelector`, `RingBufferTrackingSymbolUpdates`, `TicksCounter` (15 source files)
- **Build**: `yarn build-force` (tsup) — normal build disabled for tsconfig path aliases
- **No barrel file** — import directly from individual files
- **Used for**: Real-time market data processing, buffering, and symbol update tracking
