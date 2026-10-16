// Actual installed Axios/axios-retry pipeline, with a deterministic adapter,
// wall clock and timer leaf. No copy of its retry algorithm or config reducer.
const axios = require('axios');
const axiosRetry = require('axios-retry').default;
const scenarios = JSON.parse(process.argv[2]);
(async () => {
  const all = [];
  const realNow = Date.now, realTimer = global.setTimeout;
  try {
    for (const scenario of scenarios) {
      let now = 1000, attempts = 0;
      const trace = [];
      Date.now = () => now;
      global.setTimeout = (callback, ms) => {
        trace.push({kind:'sleep', ms, time:now}); now += ms;
        queueMicrotask(callback); return 1;
      };
      const xhr = axios.create({timeout:scenario.timeout, adapter: async config => {
        trace.push({kind:'send', count:config['axios-retry'].retryCount, timeout:config.timeout, time:now});
        now += 20;
        const response = {status:attempts++ < scenario.failures ? 429 : 200, data:{}, headers:{}, config};
        if (response.status !== 200) throw new axios.AxiosError('scripted rate limit', 'ERR_BAD_RESPONSE', config, undefined, response);
        return response;
      }});
      axiosRetry(xhr, {
        retries:3,
        shouldResetTimeout:scenario.reset,
        retryCondition:error => {
          const count = error.config['axios-retry'].retryCount;
          trace.push({kind:'condition', count, time:now});
          if (scenario.nullCondition) return null;
          if (scenario.asyncApproval) return Promise.resolve(undefined);
          if (scenario.asyncRejection) return Promise.reject(new Error('predicate refusal'));
          return scenario.asyncRefusal ? Promise.resolve(false) : count < scenario.stop;
        },
        retryDelay:(count,error) => {
          trace.push({kind:'delay', count, status:error.response.status, time:now});
          return scenario.delay * count;
        },
      });
      const config = {};
      if (scenario.seed !== null) config['axios-retry'] = {retryCount:scenario.seed, retries:2};
      let outcome;
      try { outcome = {ok:true, count:(await xhr.get('/retry',config)).config['axios-retry'].retryCount}; }
      catch(error) { outcome = {ok:false, count:error.config['axios-retry'].retryCount}; }
      all.push({trace,outcome});
    }
  } finally { Date.now = realNow; global.setTimeout = realTimer; }
  process.stdout.write(JSON.stringify(all));
})().catch(error => { console.error(error); process.exitCode=1; });
