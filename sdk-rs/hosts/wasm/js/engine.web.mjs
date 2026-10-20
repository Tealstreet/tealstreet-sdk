// Keep the package entry available before a WASM build. The literal import
// lets browser bundlers include the generated engine when it is needed.
export async function loadEngine() {
  const engine = await import('../pkg/web/safe_cex_wasm.js');
  await engine.default();
  return engine;
}
