/**
 * Go's `wasm_exec.js` runtime glue is a plain script (it assigns the global
 * `Go`), not an ES module. This sidecar declaration lets tsc resolve the
 * side-effect `import('./wasm/wasm_exec.js')` in the loaders — needed now that
 * consumers (e.g. @tealstreet/lighter-sdk) follow poseidon-crypto's source via
 * the literal loader imports in index.ts.
 */
export {};
