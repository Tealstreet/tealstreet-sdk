# CLAUDE.md — @tealstreet/json-parse

Safe JSON parsing utilities that handle large integers (16+ digits) without precision loss.

- **Key file**: `jsonParse.ts`
- **Exports**: `jsonParse()` (safe with null fallback), `bigIntAsStringJsonParse()` (converts big integers to strings before parsing)
- **Build**: `yarn build-force` (tsup)
- **Note**: Uses pre-compiled regex for performance. Prevents JavaScript's floating-point precision issues with exchange order IDs and timestamps.
