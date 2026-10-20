// Optional instrumentation only: normal replay and performance gates do not
// import this module. Gross CPU includes clock overhead and wasm-bindgen
// argument/return handling; it must not be treated as an exact decomposition.
export function profileEngine(engine, { cpuSource = process, wallNow = () => performance.now() } = {}) {
  const threadClock = typeof cpuSource.threadCpuUsage === 'function';
  const cpuUsage = (threadClock ? cpuSource.threadCpuUsage : cpuSource.cpuUsage).bind(cpuSource);
  const cpuClock = threadClock ? 'threadCpuUsage' : 'cpuUsage (process fallback; includes other threads)';
  const methods = {};
  const wrapped = new Proxy(engine, {
    get(target, key) {
      const value = Reflect.get(target, key, target);
      if (typeof value !== 'function') return value;
      return (...args) => {
        const start = wallNow();
        const cpuStart = cpuUsage();
        try { return Reflect.apply(value, target, args); }
        finally {
          const cpu = cpuUsage(cpuStart);
          const wallMs = wallNow() - start;
          const row = methods[key] ??= { calls: 0, wallMs: 0, cpuMs: 0, cpuClock };
          row.calls++;
          row.wallMs += wallMs;
          row.cpuMs += (cpu.user + cpu.system) / 1000;
        }
      };
    },
  });
  return { engine: wrapped, methods, cpuClock };
}
