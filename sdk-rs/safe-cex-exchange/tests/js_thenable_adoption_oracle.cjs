// Independent engine observations. No replacement resolver or Promise.
(async () => {
  const modes = ['fulfill', 'nan', 'reject', 'getter-throw', 'call-throw',
    'resolve-then-throw', 'reject-then-resolve', 'nested', 'noncallable', 'pending-first'];
  const output = {};
  for (const route of ['await', 'return', 'all', 'allSettled', 'resolve', 'then', 'finally']) {
    output[route] = [];
    for (const mode of modes) {
      const trace = [];
      const token = { unit: '\ud800' };
      function make(label, kind) {
        return { get then() {
          trace.push(`get:${label}`);
          if (kind === 'getter-throw') throw token;
          if (kind === 'noncallable') return 7;
          const owner = this;
          return function(resolve, reject) {
            trace.push(`call:${label}:${this === owner}`);
            if (kind === 'reject' || kind === 'reject-then-resolve') {
              reject(token); if (kind === 'reject-then-resolve') resolve(-0);
            } else if (kind === 'call-throw') throw token;
            else if (kind === 'nested') resolve(make('inner', 'fulfill'));
            else if (kind === 'pending-first') {
              let settle;
              const pending = new Promise(r => { settle = r; });
              resolve(pending); reject(token);
              Promise.resolve().then(() => settle(-0));
            } else { resolve(kind === 'nan' ? NaN : -0); if (kind === 'resolve-then-throw') throw token; }
          };
        }};
      }
      const input = make('outer', mode);
      let observed;
      function project(ok, value) {
        return { ok, same: value === (ok ? input : token),
          bits: typeof value === 'number' ? (Object.is(value, -0) ? '8000000000000000' : Number.isNaN(value) ? 'nan' : 'other') : null,
          units: !ok && typeof value.unit === 'string' ? Array.from({length:value.unit.length}, (_, i) => value.unit.charCodeAt(i)) : null };
      }
      if (route === 'await') {
        const driver = (async () => { try { observed = project(true, await input); } catch(e) { observed = project(false,e); } })();
        trace.push('returned'); await driver;
      } else {
        let promise;
        if (route === 'return') promise = (async () => input)();
        if (route === 'resolve') promise = Promise.resolve(input);
        if (route === 'then') promise = Promise.resolve().then(() => input);
        if (route === 'finally') promise = Promise.resolve(-0).finally(() => input);
        if (route === 'all' || route === 'allSettled') promise = Promise[route]([input]);
        trace.push('returned');
        try {
          let value = await promise;
          if (route === 'all') value = value[0];
          if (route === 'allSettled') {
            if (value[0].status === 'rejected') throw value[0].reason;
            value = value[0].value;
          }
          observed = project(true,value);
        } catch(e) { observed = project(false,e); }
      }
      output[route].push({mode, trace, observed});
    }
  }
  output.iterators = [];
  for (const method of ['all', 'allSettled']) for (const abrupt of [false, true]) {
    const trace = [], token = {unit:'\ud800'};
    let index = 0;
    const iterable = { [Symbol.iterator]() { trace.push('iterator'); return {
      next() {
        const i = index++; trace.push(`next:${i}`);
        if (i === 1 && abrupt) throw token;
        if (i === 2) return {done:true};
        return {done:false, value:{get then() {trace.push(`get:${i}`); return resolve => {trace.push(`call:${i}`);resolve(i);};}}};
      }, return() {trace.push('return');return {};}
    };}};
    const promise = Promise[method](iterable); trace.push('returned');
    let same = false;
    try { await promise; } catch(e) { same = e === token; }
    output.iterators.push({method,abrupt,trace,same});
  }
  output.fifo = [];
  for (const kind of ['scalar', 'promise']) {
    const trace = [];
    const value = kind === 'scalar' ? -0 : Promise.resolve(-0);
    const body = (async () => {trace.push('body');await value;trace.push('resumed');})();
    Promise.resolve().then(() => trace.push('reaction'));
    trace.push('returned'); await body;
    output.fifo.push({kind,trace});
  }
  output.turns = [];
  for (const route of ['await', 'return']) for (const kind of ['scalar', 'promise', 'thenable']) {
    const trace = [];
    const value = kind === 'scalar' ? -0 : kind === 'promise' ? Promise.resolve(-0) : {
      get then() { trace.push('get'); return resolve => { trace.push('call'); resolve(-0); }; }
    };
    const body = route === 'await'
      ? (async () => { trace.push('body'); await value; trace.push('resumed'); })()
      : (async () => { trace.push('body'); return value; })();
    const done = body.then(() => { trace.push('done'); });
    let tail = Promise.resolve();
    for (const label of ['q1','q2','q3']) tail = tail.then(() => { trace.push(label); });
    trace.push('returned'); await done; await tail;
    output.turns.push({route,kind,trace});
  }
  process.stdout.write(JSON.stringify(output));
})().catch(error => {console.error(error);process.exitCode=1;});
