const worker = new Worker('/smoke/web/worker.mjs', { type: 'module' });
const finish = result => {
  window.ffiSmokeResult = result;
  worker.terminate();
};
worker.onmessage = event => finish(event.data);
worker.onerror = event => finish({ error: event.message || 'Browser Worker module failed to load or execute; check worker imports and wasm assets.' });
worker.onmessageerror = () => finish({ error: 'Worker message decoding failed' });
