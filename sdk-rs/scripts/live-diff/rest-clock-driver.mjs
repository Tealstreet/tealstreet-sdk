import { setTimeout as delay } from 'node:timers/promises';

// Never expire an in-flight exchange HTTP request merely to progress a limiter.
// Child calls retain their independent wall-clock watchdogs.
export async function driveOperation(s, spec, { realtimeClock = false, wait = () => delay(500) } = {}) {
  let reply,
    finished = false;
  const clock = [s.atMs];
  const operation = s.call(spec.method, spec.args).then(
    (value) => {
      reply = value;
      finished = true;
    },
    (error) => {
      reply = {
        probeFailure: true,
        timeout: error.timeout === true,
        notRun: error.notRun === true,
        failure: { name: 'ProbeError', message: error.message },
      };
      finished = true;
    },
  );
  while (!finished) {
    await Promise.race([operation, wait()]);
    if (finished || realtimeClock || s.records.some((r) => r.status === undefined && !r.blocked && !r.transportError))
      continue;
    try {
      await s.call('$advance', [s.atMs + 1000]);
      s.atMs += 1000;
      clock.push(s.atMs);
    } catch (error) {
      await operation;
      break;
    }
  }
  await operation;
  reply.clock = clock;

  return reply;
}
