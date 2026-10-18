const http = require('node:http');
const axios = require('axios');
const axiosRetry = require('axios-retry').default;

(async () => {
  let requests = 0;
  const server = http.createServer((request, response) => {
    requests++;
    response.writeHead(requests === 1 ? 429 : 200, {'Content-Type': 'application/json'});
    response.end('{}');
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  try {
    const capture = {};
    const client = axios.create({baseURL: `http://127.0.0.1:${server.address().port}`, timeout: 5000, proxy: false});
    let dispatched = 0;
    const adapter = axios.getAdapter(client.defaults.adapter);
    client.defaults.adapter = config => { dispatched++; return adapter(config); };
    axiosRetry(client, {
      retries: 1,
      retryCondition: () => true,
      retryDelay: () => { capture.beforeReplay = requests; return 25; },
    });
    let completed;
    const done = new Promise((resolve, reject) => { completed = {resolve, reject}; });
    client.get('/retry').then(completed.resolve, completed.reject);
    capture.immediate = dispatched;
    await done;
    capture.afterReplay = requests;
    process.stdout.write(JSON.stringify(capture));
  } finally {
    await new Promise(resolve => server.close(resolve));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
