# CLAUDE.md — @tealstreet/compression

Data compression/decompression using the native browser `CompressionStream` API.

- **Key files**: `compress.ts`, `decompress.ts`
- **Build**: `yarn build-force` (tsup)
- **Note**: Async gzip functions. No external compression dependencies — uses native `CompressionStream` API (browser/Node.js 16+).
