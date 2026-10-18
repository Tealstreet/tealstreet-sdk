// Native String conversion, observed as UTF-16 before JSON serialization.
const cases = [
  'plain', 'string', 'number', 'undefined', 'null', 'bigint', 'negative-zero', 'utf16',
  'object-fallback', 'noncallable-fallback', 'both-objects', 'both-noncallable',
  'undefined-shadow', 'first-throws', 'second-throws', 'first-getter-throws',
  'first-getter', 'second-getter', 'second-getter-unreached', 'mutate-second',
  'delete-second', 'inherited', 'only-valueof', 'null-prototype', 'function-hook',
  'array-child', 'error-name', 'stock-prototype', 'stock-shadow', 'stock-class', 'stock-error',
];
const rows = cases.map(label => {
  const events = [];
  let value = {sentinel:'receiver'};
  const fn = (name, result, throwing) => function() {
    if (this !== value) throw new Error('wrong receiver');
    events.push(name);
    if (throwing) throw new RangeError(name);
    return result;
  };
  const first = result => value.toString = fn('toString',result);
  const second = result => value.valueOf = fn('valueOf',result);
  switch(label) {
    case 'plain': break;
    case 'string': first('hook'); second(99); break;
    case 'number': first(42); break;
    case 'undefined': first(undefined); break;
    case 'null': first(null); break;
    case 'bigint': first(45n); break;
    case 'negative-zero': first(-0); break;
    case 'utf16': first('\ud800x'); break;
    case 'object-fallback': first({}); second('fallback'); break;
    case 'noncallable-fallback': value.toString=7; second('fallback'); break;
    case 'both-objects': first({}); second({}); break;
    case 'both-noncallable': value.toString=7; value.valueOf=null; break;
    case 'undefined-shadow': value.toString=undefined; break;
    case 'first-throws': value.toString=fn('toString',null,true); second('unreached'); break;
    case 'second-throws': first({}); value.valueOf=fn('valueOf',null,true); break;
    case 'first-getter-throws': Object.defineProperty(value,'toString',{get(){events.push('get-toString');throw new SyntaxError('get-toString');}}); break;
    case 'first-getter': Object.defineProperty(value,'toString',{get(){events.push('get-toString');return fn('toString','getter');}}); break;
    case 'second-getter': first({}); Object.defineProperty(value,'valueOf',{get(){events.push('get-valueOf');return fn('valueOf',true);}}); break;
    case 'second-getter-unreached': first('done'); Object.defineProperty(value,'valueOf',{get(){throw new Error('unreached');}}); break;
    case 'mutate-second': value.toString=function(){events.push('toString');second('mutated');return {};}; second('stale'); break;
    case 'delete-second': value.toString=function(){events.push('toString');delete this.valueOf;return {};}; second('stale'); break;
    case 'inherited': Object.setPrototypeOf(value,{toString:fn('toString','inherited')}); break;
    case 'only-valueof': second(81); break;
    case 'null-prototype': Object.setPrototypeOf(value,null); break;
    case 'function-hook': value=function sample(){}; first('function-hook'); break;
    case 'array-child': { const child={toString(){events.push('child-toString');return 'child';}};value=['left',child,null];break; }
    case 'stock-prototype': Object.setPrototypeOf(value,Object.prototype);break;
    case 'stock-shadow': Object.setPrototypeOf(value,Object.create(Object.prototype,{toString:{value:undefined}}));break;
    case 'stock-class': Object.setPrototypeOf(value,Object.prototype);break;
    case 'stock-error': value=new Error('detail');Object.setPrototypeOf(value,Object.prototype);break;
    case 'error-name': value=new Error('detail');value.name={toString(){events.push('name-toString');return 'Named';}};break;
    default: throw new Error(label);
  }
  try {
    const result=String(value);
    return {label,events,units:Array.from({length:result.length},(_,i)=>result.charCodeAt(i))};
  } catch(error) { return {label,events,error:error.name,message:error.message}; }
});
process.stdout.write(JSON.stringify(rows));
