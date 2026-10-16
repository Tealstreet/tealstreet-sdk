const vm = require('node:vm');
const names = ['constructor','__defineGetter__','hasOwnProperty','toString','valueOf','__proto__'];
const output = names.map(name => {
  const context = vm.createContext({});
  return vm.runInContext(`(() => {
    const first = ({})[${JSON.stringify(name)}], second = ({})[${JSON.stringify(name)}];
    const before = [typeof first, String(first), String(first.length), first === second];
    first.late = 7;
    return { before, late: second.late, keys: Object.keys(second) };
  })()`, context);
});
process.stdout.write(JSON.stringify(output));
