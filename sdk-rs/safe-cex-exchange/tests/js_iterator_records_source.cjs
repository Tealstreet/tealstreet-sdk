'use strict';
const cases = [];
for (const fault of ['next','done','value','body']) {
  const trace = [], token = {}, cleanup = {};
  const iterator = {
    get next() {
      trace.push('getNext');
      return function next() {
        trace.push('next');
        if (fault === 'next') throw token;
        return {
          get done() { trace.push('done'); if (fault === 'done') throw token; return false; },
          get value() { trace.push('value'); if (fault === 'value') throw token; return 1; }
        };
      };
    },
    return() { if (this !== iterator) throw new Error('return receiver'); trace.push('return'); throw cleanup; }
  };
  try { Array.from({[Symbol.iterator](){return iterator;}}, () => {trace.push('body');throw token;}); }
  catch (error) { trace.push(error === token ? 'original' : 'other'); }
  cases.push({fault,trace});
}
console.log(JSON.stringify(cases));
