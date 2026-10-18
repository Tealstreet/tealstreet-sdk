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
if (process.argv[2] === 'contract') {
  const inputs = [[],[7,'😀',null],'A😀',7,null];
  const record = inputs.map(input => {
    try {
      const iterator = input[Symbol.iterator]();
      const values = [];
      for (const value of iterator) {
        if (typeof value === 'number') { const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);values.push({bits:bytes.toString('hex')}); }
        else if (typeof value === 'string') values.push({units:Array.from({length:value.length},(_,i)=>value.charCodeAt(i))});
        else if (value === null) values.push({null:true});
        else throw new Error('unreviewed fixture value');
      }
      return {input,output:{values,exhausted:iterator.next().done,closed:iterator.return === undefined}};
    } catch(error) { return {input,output:{error:error.name}}; }
  });
  console.log(JSON.stringify({record}));
}
else if (process.argv[2] !== 'normal') console.log(JSON.stringify(cases));
else {
  const results = [];
  for (const mode of ['object','primitive','throw','getThrow','undefined','number','class']) {
    const trace = [], token = {}, cleanup = {}, skipped = {};
    let at = 0;
    const iterator = {};
    Object.defineProperty(iterator,'next',{configurable:true,get(){
      trace.push('getNext');
      return function next(){
        if (this !== iterator) throw new Error('next receiver');
        const index = at++;
        trace.push('next'+index);
        if (index === 0) Object.defineProperty(iterator,'next',{value(){throw new Error('uncached next');},configurable:true});
        return {
          get done(){trace.push('done'+index);return false;},
          get value(){trace.push('value'+index);if(index===0)throw skipped;return token;}
        };
      };
    }});
    Object.defineProperty(iterator,'return',{get(){
      trace.push('getReturn');
      if (mode === 'getThrow') throw cleanup;
      if (mode === 'undefined') return undefined;
      if (mode === 'number') return 7;
      if (mode === 'class') return class Return {};
      return function close(){
        if(this !== iterator)throw new Error('return receiver');
        trace.push('return');
        if(mode==='throw')throw cleanup;
        return mode==='object' ? {} : 7;
      };
    }});
    let boundSame = false, failure = 'none';
    try { const [,bound] = {[Symbol.iterator](){trace.push('getIterator');return iterator;}}; boundSame = bound===token; }
    catch(error){failure=error===cleanup?'cleanup':error instanceof TypeError?'TypeError':'other';}
    results.push({mode,trace,boundSame,failure});
  }
  console.log(JSON.stringify(results));
}
