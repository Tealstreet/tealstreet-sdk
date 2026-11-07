// Minimal CDP transport for the measurement job; no browser library dependency.
export class Cdp {
  constructor(socket, timeoutMs = 30_000) {
    this.socket = socket;
    this.pending = new Map();
    this.next = 0;
    this.closed = false;
    this.timeoutMs = timeoutMs;
    this.ready = new Promise((resolve, reject) => {
      this.rejectReady = reject;
      const timer = setTimeout(() => this.fail(new Error('CDP connection timed out')), timeoutMs);
      this.readyTimer = timer;
      socket.addEventListener('open', () => { clearTimeout(timer); resolve(); }, { once: true });
      if (socket.readyState === 1) { clearTimeout(timer); resolve(); }
    });
    // Disposal/errors may arrive before the first command awaits readiness.
    this.ready.catch(() => {});
    socket.addEventListener('error', () => this.fail(new Error('CDP socket failed')));
    socket.addEventListener('close', () => this.fail(new Error('CDP socket closed')));
    socket.addEventListener('message', ({ data }) => {
      let message;
      try { message = JSON.parse(data); }
      catch { this.fail(new Error('Malformed CDP message')); return; }
      if (!message || typeof message !== 'object' || Array.isArray(message)) {
        this.fail(new Error('Malformed CDP message')); return;
      }
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      clearTimeout(pending.timer);
      if (message.error) pending.reject(new Error(`CDP: ${message.error.message ?? 'command failed'}`));
      else pending.resolve(message.result);
    });
  }

  fail(error) {
    this.closed = true;
    clearTimeout(this.readyTimer);
    this.rejectReady(error);
    for (const pending of this.pending.values()) { clearTimeout(pending.timer); pending.reject(error); }
    this.pending.clear();
  }

  async send(method, params = {}) {
    await this.ready;
    if (this.closed) throw new Error('CDP socket is closed');
    const id = ++this.next;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`CDP ${method} timed out`));
      }, this.timeoutMs);
      this.pending.set(id, { resolve, reject, timer });
      try { this.socket.send(JSON.stringify({ id, method, params })); }
      catch (error) { clearTimeout(timer); this.pending.delete(id); reject(error); }
    });
  }

  close() { this.fail(new Error('CDP disposed')); this.socket.close(); }
}

export async function evaluateWorker(cdp, layout, venue) {
  const result = await cdp.send('Runtime.evaluate', {
    awaitPromise: true,
    returnByValue: true,
    expression: `new Promise((resolve, reject) => {
      const start = performance.now();
      const worker = new Worker('/browser-worker.mjs', { type: 'module' });
      const timer = setTimeout(() => finish(new Error('Worker readiness timed out')), 25000);
      let bootstrapMs;
      const finish = (error, value) => {
        clearTimeout(timer); worker.terminate();
        if (error) reject(error); else resolve(value);
      };
      worker.onerror = (event) => finish(new Error(event.message || 'Worker failed'));
      worker.onmessageerror = () => finish(new Error('Worker message could not be decoded'));
      worker.onmessage = ({ data }) => {
        if (data.phase === 'bootstrap') {
          bootstrapMs = performance.now() - start;
          worker.postMessage(${JSON.stringify({ layout, venue })});
        } else if (data.error) finish(new Error(data.error));
        else finish(undefined, { ...data, bootstrapMs, roundTripMs: performance.now() - start });
      };
    })`,
  });
  if (result?.exceptionDetails) throw new Error(`Browser evaluation failed: ${result.exceptionDetails.exception?.description ?? result.exceptionDetails.text}`);
  if (!result?.result || !Object.hasOwn(result.result, 'value')) throw new Error('Browser returned no measurement');
  return result.result.value;
}
