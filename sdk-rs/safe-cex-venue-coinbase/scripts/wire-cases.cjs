function sample(s, prefix = "sample", sequence = { value: 0 }) {
  switch (s.kind) {
    case 'string': return `sample:${prefix}`;
    case 'number': return ++sequence.value + 0.25;
    case 'boolean': return sequence.value++ % 2 === 0;
    case 'enum': return s.values[sequence.value++ % s.values.length];
    case 'array': return [sample(s.item, `${prefix}[0]`, sequence)];
    case 'object': return Object.fromEntries(s.fields.map(f => [f.name, sample(f.schema, `${prefix}.${f.name}`, sequence)]));
    default: throw Error(s.kind);
  }
}
function cases(s, prefix) {
  const base = sample(s, prefix), result = [{ name: `${prefix}: complete`, value: base, valid: true }];
  const add = (name, value, valid) => result.push({ name: `${prefix}: ${name}`, value, valid });
  add('null rejected', null, false);
  if (s.kind === 'enum') {
    for (const v of s.values) {
      add(`literal ${v}`, v, true);
      if (typeof v === "boolean") add("opposite boolean", !v, false);
      if (typeof v === "string") add("externally tagged object rejected", {[v]: null}, false);
    }
    add('unknown literal rejected', '__not_a_literal__', false);
    add('number rejected', 1, false);
  } else if (s.kind === 'object') {
    add('array rejected', [], false);
    add('complete positional array rejected', s.fields.map(f => sample(f.schema, `${prefix}.${f.name}`)), false);
    add('primitive rejected', 'invalid', false);
    add('all optional absent', Object.fromEntries(s.fields.filter(f => !f.optional).map(f => [f.name, sample(f.schema, `${prefix}.${f.name}`)])), true);
    for (const f of s.fields) {
      const absent = { ...base }; delete absent[f.name];
      add(`${f.name} missing`, absent, f.optional);
      for (const c of cases(f.schema, f.name)) add(c.name, { ...base, [f.name]: c.value }, c.valid);
    }
  } else if (s.kind === 'array') {
    add('empty array', [], true);
    add('object rejected', {}, false);
    for (const c of cases(s.item, '[0]')) add(c.name, [c.value], c.valid);
  } else {
    const probes = { string: [12, true, {}, []], number: ['12', false, {}, []], boolean: ['true', 1, {}, []] };
    for (const value of probes[s.kind]) add(`wrong primitive ${JSON.stringify(value)}`, value, false);
  }
  return result;
}

module.exports = { cases };
