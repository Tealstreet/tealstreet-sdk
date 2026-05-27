# CLAUDE.md — @tealstreet/poseidon-crypto

WebAssembly-based Poseidon2 cryptography library providing hash functions and Schnorr signatures. Used by the Lighter DEX SDK.

## Architecture

```text
src/
├── index.ts            # Public API: initWasm(), getCryptoApi(), isWasmReady()
├── types.ts            # PoseidonCrypto, Poseidon2Hasher, SchnorrSigner interfaces
├── loader.ts           # Unified WASM loader logic
├── browser-loader.ts   # Browser loader (fetches from /public/wasm/)
├── node-loader.ts      # Node.js loader (reads from filesystem)
└── wasm/
    ├── main.go                 # Go source compiled to WASM
    ├── poseidon_crypto.wasm    # Binary WASM artifact
    └── wasm_exec.js            # Go runtime glue code
```

**Usage pattern**: Call `initWasm()` once → `getCryptoApi()` to get the API → use hash/sign functions.

**API**:

- `poseidon2Hash(input)` — Poseidon2 hash (`HashNoPad`, 32-byte output)
- `poseidon2HashFp5(input)` — Poseidon2 hash-to-quintic-extension (`HashToQuinticExtension`, 40-byte output)
- `poseidon2HashTwoToOne(h1, h2)` — Poseidon2 two-to-one helper
- `schnorrGenerateKeys()` / `schnorrSign(message, privateKey)` / `schnorrVerify(...)` — Schnorr signatures (ECgFp5 curve)

**Data format**: All inputs/outputs are hex-encoded strings. Hash inputs must be multiples of 8 bytes (16 hex chars).

## Commands

```bash
yarn build-force    # Build with tsup (ESM only)
yarn test           # Vitest (Node.js environment)
```

**Rebuilding WASM** (requires Go):

```bash
cd src/wasm && GOOS=js GOARCH=wasm go build -o poseidon_crypto.wasm main.go
```

After rebuilding package-local WASM, copy it to the web static path so browser runtime matches SDK expectations:

```bash
cp src/wasm/poseidon_crypto.wasm ../../apps/web/public/wasm/poseidon_crypto.wasm
```

## Gotchas

- **Typecheck and lint disabled** — WASM toolchain incompatibilities
- **Build disabled by default** — uses tsconfig path aliases
- `initWasm()` must be called exactly once before `getCryptoApi()`
- Browser: WASM must be served from `/public/wasm/` (Next.js static files)
- Browser + SDK must stay in sync: if loaders expect `poseidon2HashFp5` but deployed WASM is old, runtime will fail and Lighter signatures can break.
- Loader fallback (`poseidon2HashFp5 ?? poseidon2Hash`) is compatibility-only and should not be treated as correct for fp5-specific signing flows.
- Functions return `WasmError` on failure — always check return types
- Promise caching prevents duplicate initialization
- **Loader imports must use literal specifiers**, not a variable. `initWasm()`
  picks the browser/node loader with `isBrowserLikeRuntime() ? import('./browser-loader') : import('./node-loader')`. A variable specifier (`import(loaderPath)`) is not statically resolvable by vite/vitest and broke `@tealstreet/lighter-sdk`'s WASM init when poseidon-crypto is imported across the package boundary (source consumption). The unused branch is code-split out at build.
- **`wasm/wasm_exec.d.ts`** exists only so consumers that follow this source (lighter-sdk's tsc) can resolve the side-effect `import('./wasm/wasm_exec.js')` — the Go glue is a script, not an ES module. Don't delete it.
