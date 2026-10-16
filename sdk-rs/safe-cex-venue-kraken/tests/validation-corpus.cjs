globalThis.validationCorpus = async () => {
  const corpus = {};
  const balance = {free: 11.25, used: 17.5, total: 29.75, upnl: -31.125,
    currencies: {USD: {free: 37.25, used: 41.5, total: 43.75, upnl: -47.125,
      info: {tag: 'currency'}}}, info: {tag: 'balance'}};
  for (const family of ['futures', 'spot']) {
    const specs = [];
    const add = (label, data, extra = {}) => specs.push({label, raw: JSON.stringify(data) ?? null, ...extra});
    const good = family === 'futures' ? {result: 'success', accounts: {cash: {}}} : {error: []};
    for (const data of [undefined, null, false, 0, 'primitive', {}, [],
      {error: []}, {error: null}, {error: false}, {error: 'invalid key'},
      {error: {length: 1}}, {error: ['EAPI:Invalid key']}, {error: ['EAPI:Invalid nonce']},
      {error: ['Invalid signature']}, {error: ['permission denied']}, {error: ['rate limit']},
      {error: ['Invalid key', 'Invalid nonce']}, {error: [null, false, 17.25, ['nested'], {}]},
      {error: [{toString: 7, valueOf: 9}]}, {result: 'success'},
      {result: 'success', accounts: {}}, {result: 'success', accounts: {cash: {}}}]) {
      add('shape-' + specs.length, data);
    }
    for (const result of ['APIKEY inactive', 'nonce', 'denied', 'authentication', 'IP mismatch', 'unknown refusal']) {
      add('result-' + result, {result});
    }
    for (const accounts of [false, 0, [], [1], 'x', 1, {}]) add('accounts-' + specs.length, {result: 'success', accounts});
    for (const data of [{error: [17]}, {result: 17}, {error: {'0': 'Invalid key'}}, {error: {length: '0'}},
      {error: ['', 'invalid key']}, {error: [[1, 2]]}]) add('malformed-' + specs.length, data);
    for (const mode of ['sync', 'async']) {
      for (const status of [undefined, null, '401', 401, 403, 429, 400, 500]) {
        add('status-' + mode + '-' + status, good, {mode, message: 'network timeout', response: {status}});
      }
    }
    for (const message of ['', 'other', 'CORS', 'NETWORK', 'TIMEOUT']) add('message-' + message, good, {mode: 'async', message});
    for (const response of [{data: {error: ['response refusal']}}, {data: {error: 'abc'}},
      {data: {error: 9}}, {data: {error: {why: 'bad'}}}, {data: {message: 'cors response'}},
      {data: {error: [], message: 'ignored message'}}]) add('response-' + specs.length, good, {mode: 'async', message: 'original', response});
    for (const status of [401, 403, 429, 400]) {
      add('astral-status-' + status, good, {mode: 'async', message: 'original', response: {status, data: {error: '😀'}}});
    }
    for (const balanceMode of ['sync', 'async', 'late']) add('balance-' + balanceMode, good, {balanceMode});
    const cases = [];
    for (const spec of specs) {
      const calls = [], trace = [];
      const original = new Error(spec.message ?? 'read failure');
      if (spec.response) original.response = spec.response;
      let release;
      const pending = new Promise(resolve => {release = resolve;});
      const read = (method, endpoint, body) => {
        calls.push({method, endpoint, body: capture(body)}); trace.push('read');
        if (spec.mode === 'sync') throw original;
        if (spec.mode === 'async') return Promise.reject(original);
        return pending;
      };
      const receiver = {xhr: {get: endpoint => read('GET', endpoint, undefined),
        post: (endpoint, body) => read('POST', endpoint, body)}, fetchBalance() {
          trace.push('balance');
          if (spec.balanceMode === 'sync') throw new Error('balance failure');
          if (spec.balanceMode === 'async') return Promise.reject(new Error('balance failure'));
          return Promise.resolve(balance);
        }};
      const fn = method(family === 'futures' ? futuresFile : spotFile,
        family === 'futures' ? 'KrakenExchange' : 'KrakenSpotExchange', 'validateAccount',
        {ENDPOINTS: family === 'futures' ? futuresTypes.ENDPOINTS : spotTypes.ENDPOINTS}, receiver);
      const resultPromise = fn();
      const prefixTrace = trace.slice();
      if (spec.balanceMode === 'late') receiver.fetchBalance = () => {
        trace.push('late-balance'); return Promise.resolve({...balance, free: 53.625});
      };
      release({data: value(spec.raw)});
      try {
        cases.push({...spec, calls, trace, prefixTrace, result: capture(await resultPromise)});
      } catch (error) {
        cases.push({...spec, calls, trace, prefixTrace, thrown: {name: error.name, message: error.message}});
      }
    }
    corpus[family] = cases;
  }
  const boundaries = [];
  for (const origin of ['envelope', 'catch']) {
    const raw = JSON.stringify({error: '😀'});
    const error = new Error('original'); error.response = {data: {error: '😀'}};
    const receiver = {xhr: {get: async () => {
      if (origin === 'catch') throw error;
      return {data: JSON.parse(raw)};
    }}, fetchBalance() {throw Error('unexpected boundary balance call');}};
    const fn = method(futuresFile, 'KrakenExchange', 'validateAccount', {ENDPOINTS: futuresTypes.ENDPOINTS}, receiver);
    const result = await fn();
    boundaries.push({origin, raw, units: Array.from({length: result.error.length}, (_, i) => result.error.charCodeAt(i)),
      hasBalance: Object.hasOwn(result, 'balance')});
  }
  corpus.futuresUtf16Boundaries = boundaries;
  return corpus;
};
