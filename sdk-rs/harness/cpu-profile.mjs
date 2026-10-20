// Optional local V8 sampling. Session.connect() attaches to this process without
// opening an inspector port; ordinary replay never imports the inspector API.
export async function startCpuProfile(path, { createSession, writeProfile } = {}) {
  if (!path) return undefined;
  const session = createSession ? await createSession() : new (await import('node:inspector/promises')).Session();
  try {
    session.connect();
    await session.post('Profiler.enable');
    await session.post('Profiler.setSamplingInterval', { interval: 1000 });
    await session.post('Profiler.start');
  } catch (error) {
    session.disconnect();
    throw error;
  }
  let stopping;
  return {
    stop() {
      return stopping ??= (async () => {
        let result;
        try { result = await session.post('Profiler.stop'); }
        finally { session.disconnect(); }
        if (writeProfile) await writeProfile(path, result.profile);
        else {
          const { writeFile } = await import('node:fs/promises');
          await writeFile(path, JSON.stringify(result.profile));
        }
      })();
    },
  };
}
