import { auditFixerChanges } from './fixer-contracts.mjs';

// Observe each write, rather than only a net before/after diff: a helper cannot
// hide an undeclared overwrite by restoring the old value before returning.
// This runs only on correctness output, outside every performance subprocess.
export function runAuditedFixer(venue, reference, fix) {
  const errors = [];
  const counts = {};
  let total = 0;
  const initial = structuredClone(reference);
  const originals = new WeakMap();
  const unwrap = (value) => {
    if (!value || typeof value !== 'object') return value;
    if (originals.has(value)) return originals.get(value);
    const entries = Object.entries(value).map(([key, item]) => [key, unwrap(item)]);
    // Detach newly supplied objects so a local unobserved alias cannot mutate
    // reference state after assignment. Already observed rows retain identity.
    return Object.assign(Array.isArray(value) ? [] : {}, Object.fromEntries(entries));
  };
  const branch = (path, value) => path.reduceRight((child, key) => {
    const parent = typeof key === 'number' ? [] : {};
    parent[key] = child;
    return parent;
  }, value);
  const observe = (target, path, operation) => {
    // Clone only the object being written, not every retained checkpoint.
    const before = structuredClone(target);
    const result = operation();
    const audit = auditFixerChanges(venue, branch(path, before), branch(path, target));
    errors.push(...audit.errors);
    total += audit.total;
    for (const [path, count] of Object.entries(audit.counts)) counts[path] = (counts[path] ?? 0) + count;
    return result;
  };
  const wrap = (value, path = []) => {
    if (!value || typeof value !== 'object') return value;
    const proxy = new Proxy(value, {
      get: (target, key, receiver) => wrap(Reflect.get(target, key, receiver), [...path,
        Array.isArray(target) && /^(0|[1-9]\d*)$/.test(String(key)) ? Number(key) : key]),
      getOwnPropertyDescriptor: (target, key) => {
        const descriptor = Reflect.getOwnPropertyDescriptor(target, key);
        if (!descriptor || !Object.hasOwn(descriptor, 'value') || !descriptor.configurable) return descriptor;
        return { ...descriptor, value: wrap(descriptor.value, [...path,
          Array.isArray(target) && /^(0|[1-9]\d*)$/.test(String(key)) ? Number(key) : key]) };
      },
      set: (target, key, next) => observe(target, path, () => Reflect.set(target, key, unwrap(next), target)),
      deleteProperty: (target, key) => observe(target, path, () => Reflect.deleteProperty(target, key)),
      // Reference rows are JSON data. Descriptor/prototype changes could hide
      // later writes from observation and have no legitimate correction use.
      defineProperty: () => { throw new Error(`${venue}: fixer changed undeclared descriptor`); },
      setPrototypeOf: () => { throw new Error(`${venue}: fixer changed undeclared prototype`); },
      preventExtensions: () => { throw new Error(`${venue}: fixer changed undeclared extensibility`); },
    });
    originals.set(proxy, value);
    return proxy;
  };
  fix(wrap(reference));
  const final = auditFixerChanges(venue, initial, reference);
  errors.push(...final.errors.filter((error) => !errors.includes(error)));
  for (const [path, count] of Object.entries(final.counts)) if (!Object.hasOwn(counts, path)) {
    counts[path] = count;
    total += count;
  }
  return { errors, counts, total };
}
