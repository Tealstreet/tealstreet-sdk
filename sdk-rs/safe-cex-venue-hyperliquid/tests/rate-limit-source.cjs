const axios = require('axios');
const rateLimit = require('axios-rate-limit');
const { setTimeout: realTimeout } = require('node:timers');
(async () => {
  const originalNow = Date.now;
  const profiles = [];
  for (const fixed of [true, false]) {
    if (fixed) Date.now = () => 1700000000000;
    else Date.now = originalNow;
    let window = 0;
    const dispatched = [];
    global.setTimeout = (callback, delay, ...args) => realTimeout(() => {
      if (delay === 20) window++;
      callback(...args);
    }, delay);
    const client = rateLimit(axios.create({adapter: async config => {
      dispatched.push([Number(config.url), window]);
      return {config, data: null, status: 200, statusText: 'OK', headers: {}};
    }}), {maxRequests: 3, perMilliseconds: 20});
    await Promise.all(Array.from({length: 13}, (_, index) => client.get(String(index))));
    if (JSON.stringify(dispatched) !== JSON.stringify(Array.from({length: 13}, (_, i) => [i, Math.floor(i / 3)])))
      throw new Error(JSON.stringify({fixed, dispatched}));
    profiles.push({fixed, dispatched});
  }
  global.setTimeout = realTimeout;
  Date.now = originalNow;
  process.stdout.write(JSON.stringify(profiles));
})().catch(error => { console.error(error); process.exitCode = 1; });
