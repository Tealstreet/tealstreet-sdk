// No static imports: the timer must include loading the production loader.
self.onmessage = async ({ data: { layout, venue } }) => {
  try {
    const start = performance.now();
    let engine;
    let imported;
    if (layout === 'families') {
      const loader = await import('/sdk-rs/hosts/wasm/js/engine.web.mjs');
      imported = performance.now();
      engine = await loader.loadEngine(venue);
    } else if (layout === 'combined') {
      engine = await import('/sdk-rs/hosts/wasm/pkg/web/safe_cex_wasm.js');
      imported = performance.now();
      await engine.default();
    } else throw new Error('Unknown measurement layout');
    if (typeof engine.WasmEngine !== 'function') throw new Error('Engine export is missing');
    const ready = performance.now();
    self.postMessage({
      realm: self.constructor.name,
      secureContext: self.isSecureContext,
      userAgent: self.navigator.userAgent,
      loaderImportMs: imported - start,
      initializeMs: ready - imported,
      moduleReadyMs: ready - start,
    });
  } catch (error) { self.postMessage({ error: String(error?.message ?? error) }); }
};
self.postMessage({ phase: 'bootstrap' });
