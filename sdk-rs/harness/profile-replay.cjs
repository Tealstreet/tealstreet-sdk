// Temporary CI diagnostic. Instrumented metrics are not performance-gate evidence.
(() => {
  const path = require('node:path');
  const args = process.argv.slice(2);
  const impl = args[args.indexOf('--impl') + 1];
  const directory = process.env.RUST_REPLAY_PROFILE_DIR;
  if (path.basename(process.argv[1] ?? '') !== 'replay.mjs' || !['rust', 'ts'].includes(impl) || !directory) return;
  if (!require('node:worker_threads').isMainThread) return;

  const { Session } = require('node:inspector');
  const { mkdirSync, writeFileSync } = require('node:fs');
  const session = new Session();
  const exit = process.exit.bind(process);
  const threadCpuUsage = process.threadCpuUsage.bind(process);
  const snapshots = new WeakMap();
  const markers = [];
  const prefix = path.join(directory, `replay-${impl}-${process.pid}`);
  let stopping = false;
  let stopped = false;
  const exitCallbacks = [];
  mkdirSync(directory, { recursive: true });
  session.connect();
  session.post('Profiler.enable');
  session.post('Profiler.setSamplingInterval', { interval: 100 });
  session.post('Profiler.start');

  // Preserve exact CPU snapshots; identity links distinguish startup and warm marks.
  process.threadCpuUsage = (previous) => {
    const at = Number(process.hrtime.bigint() / 1000n);
    const sample = threadCpuUsage(previous);
    const call = markers.length + 1;
    markers.push({ call, at, previousCall: snapshots.get(previous) ?? null, ...sample });
    snapshots.set(sample, call);
    return sample;
  };

  const stop = (after) => {
    if (stopped) return after?.();
    if (after) exitCallbacks.push(after);
    if (stopping) return;
    stopping = true;
    session.post('Profiler.stop', (error, result) => {
      let failed = false;
      try {
        if (error) throw error;
        writeFileSync(`${prefix}.cpuprofile`, JSON.stringify(result.profile));
        writeFileSync(
          `${prefix}.timings.json`,
          JSON.stringify({
            impl,
            node: process.version,
            argv: process.argv,
            samplingIntervalUs: 100,
            profileStart: result.profile.startTime,
            profileEnd: result.profile.endTime,
            markers,
          }),
        );
      } catch (error) {
        failed = true;
        process.exitCode = 1;
        process.stderr.write(`Replay profiling failed: ${error.stack ?? error}\n`);
      } finally {
        session.disconnect();
        process.threadCpuUsage = threadCpuUsage;
        stopped = true;
        for (const callback of exitCallbacks) callback(failed);
      }
    });
  };

  // replay.mjs exits explicitly, so beforeExit alone would lose the profile.
  process.exit = (code) => stop((failed) => exit(failed ? 1 : code));
  process.once('beforeExit', () => stop());
})();
