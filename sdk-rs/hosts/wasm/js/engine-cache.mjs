// All browser entry points share initialization by venue family. Concurrent
// wasm-bindgen init calls could otherwise replace the memory of a live engine.
const pending = new Map();

export function initializeWebEngine(family, importer) {
  let loading = pending.get(family);
  if (!loading) {
    loading = Promise.resolve().then(importer).then(async (engine) => {
      await engine.default();
      return engine;
    }).catch((error) => {
      if (pending.get(family) === loading) pending.delete(family);
      throw error;
    });
    pending.set(family, loading);
  }
  return loading;
}
