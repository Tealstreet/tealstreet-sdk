// Optional diagnostics only. Normal replay and performance gates do not import
// this module. The residual includes packing, action dispatch and observers;
// it is not an exact measurement of packing or all work outside the engine.
export function profileHost(host, engineProfile, { cpuSource = process, wallNow = () => performance.now() } = {}) {
  const threadClock = typeof cpuSource.threadCpuUsage === 'function';
  const cpuUsage = (threadClock ? cpuSource.threadCpuUsage : cpuSource.cpuUsage).bind(cpuSource);
  const cpuClock = threadClock ? 'threadCpuUsage' : 'cpuUsage (process fallback; includes other threads)';
  const original = host.drainMessages;
  const engineCpu = () => Object.values(engineProfile.methods).reduce((total, row) => total + row.cpuMs, 0);
  const totals = {
    outercalls: 0,
    queuedMessages: 0,
    singletonDrains: 0,
    maxMessages: 0,
    wallMs: 0,
    cpuMs: 0,
    engineCpuMs: 0,
  };
  let active = false;
  host.drainMessages = function (...args) {
    if (active || this.disposed || !this.messages.length) return Reflect.apply(original, this, args);
    // A host queue stores socket, arrival time and text in each three slots.
    // Queue counts describe timed outermost drains. Synchronous reentrant drains
    // contribute to their inclusive time without being counted/timed twice.
    const count = this.messages.length / 3;
    active = true;
    try {
      const engineStart = engineCpu();
      const start = wallNow();
      const cpuStart = cpuUsage();
      try { return Reflect.apply(original, this, args); }
      finally {
        const cpu = cpuUsage(cpuStart);
        const wallMs = wallNow() - start;
        totals.outercalls++;
        totals.queuedMessages += count;
        totals.singletonDrains += Number(count === 1);
        totals.maxMessages = Math.max(totals.maxMessages, count);
        totals.wallMs += wallMs;
        totals.cpuMs += (cpu.user + cpu.system) / 1000;
        totals.engineCpuMs += engineCpu() - engineStart;
      }
    } finally { active = false; }
  };
  return {
    cpuClock,
    // Snapshot before disposal: later cleanup cannot mutate the reported row.
    snapshot: () => Object.freeze({ ...totals, outsideEngineCpuMs: totals.cpuMs - totals.engineCpuMs, cpuClock }),
  };
}
