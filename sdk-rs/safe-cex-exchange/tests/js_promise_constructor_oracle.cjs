// ECMAScript Promise executor/resolution jobs; no replacement Promise or host.
(async () => {
  const firstTrace = [];
  const first = new Promise((resolve, reject) => {
    firstTrace.push('executor'); resolve(7); reject(8); throw { marker: 'late throw' };
  });
  firstTrace.push('returned');
  const firstValue = await first;
  const trace = [];
  const thenable = { get then() {
    trace.push('get then');
    return function (resolve, reject) {
      if (this !== thenable) throw new Error('wrong then receiver');
      trace.push('call then'); resolve(11); reject('ignored'); throw 'also ignored';
    };
  } };
  const adopted = new Promise(resolve => {
    trace.push('executor'); resolve(thenable); trace.push('after resolve');
  });
  trace.push('returned');
  const beforeJob = [...trace];
  const adoptedValue = await adopted;
  let resolve;
  const self = new Promise(r => { resolve = r; });
  resolve(self);
  const selfError = await self.catch(error => error.name);
  process.stdout.write(JSON.stringify({ firstTrace, firstValue, beforeJob, thenableTrace: trace, adoptedValue, selfError }));
})().catch(error => { console.error(error); process.exitCode = 1; });
