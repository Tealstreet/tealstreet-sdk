import init, { parity_probe, version } from '/dist/wasm/safe_cex_ffi.js';

try {
  await init();
  const values = [0, 42, 4294967295].map(n => {
    const value = parity_probe(n);
    if (value !== `safe-cex:${n}`) throw new Error(`parity_probe(${n}): ${value}`);
    return value;
  });
  postMessage({ values, version: version(), isWorker: self instanceof WorkerGlobalScope });
} catch (error) {
  postMessage({ error: String(error) });
}
