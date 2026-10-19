// Optional instrumentation only: normal replay and performance gates do not
// import this module. Durations include wasm-bindgen argument/return handling.
export function profileEngine(engine) {
  const methods = {};
  const wrapped = new Proxy(engine, {
    get(target, key) {
      const value = Reflect.get(target, key, target);
      if (typeof value !== 'function') return value;
      return (...args) => {
        const start = performance.now();
        try { return Reflect.apply(value, target, args); }
        finally {
          const row = methods[key] ??= { calls: 0, wallMs: 0 };
          row.calls++;
          row.wallMs += performance.now() - start;
        }
      };
    },
  });
  return { engine: wrapped, methods };
}
