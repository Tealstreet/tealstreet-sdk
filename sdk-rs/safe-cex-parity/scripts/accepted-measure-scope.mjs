import assert from 'node:assert/strict';
import path from 'node:path';

export function measurementScope(args, packageName) {
  const modules = args.filter(arg => arg.startsWith('--module=')).map(arg => arg.slice(9));
  const owners = args.filter(arg => arg.startsWith('--owner=')).map(arg => arg.slice(8));
  if (!modules.length && !owners.length) return undefined;
  assert.ok(args.includes('--measure') && !args.includes('--inventory'),
    'accepted-set: owner/module filters require --measure without --inventory');
  assert.ok(packageName && args.filter(arg => arg.startsWith('--package=')).length === 1,
    'accepted-set: scoped measurement requires exactly one --package');
  const prefix = `${packageName.replaceAll('-', '_')}::`;
  const qualify = name => name.startsWith(prefix) ? name : `${prefix}${name}`;
  assert.ok([...modules, ...owners].every(name => name.length && name.trim() === name),
    'accepted-set: measurement selectors must be nonempty names');
  assert.ok(modules.every(name => /^[a-zA-Z_][a-zA-Z_0-9]*(?:::[a-zA-Z_][a-zA-Z_0-9]*)*$/.test(name)),
    'accepted-set: module selectors require exact Rust module paths');
  const scope = { package: packageName, modules: modules.map(qualify), owners: owners.map(qualify) };
  for (const names of [scope.modules, scope.owners]) assert.equal(new Set(names).size, names.length,
    'accepted-set: duplicate measurement selector');
  return scope;
}

export function scopedMeasurements(packages, scope, workspace) {
  if (!scope) return packages;
  const selected = packages.filter(pkg => pkg.package === scope.package);
  assert.ok(selected.length, 'accepted-set: measurement package absent from compiler inventory');
  const names = selected.flatMap(pkg => [...pkg.types, ...(pkg.publicAliases ?? []),
    ...(pkg.defaultWitnesses ?? [])].map(owner => owner.name));
  for (const module of scope.modules) assert.ok(names.some(name => name.startsWith(`${module}::`)),
    `accepted-set: measurement module has no compiler owners: ${module}`);
  for (const owner of scope.owners) assert.ok(names.includes(owner),
    `accepted-set: measurement owner absent from compiler inventory: ${owner}`);
  const matches = owner => scope.owners.includes(owner.name)
    || scope.modules.some(module => owner.name.startsWith(`${module}::`));
  return selected.map(pkg => {
    const types = pkg.types.filter(matches);
    const publicAliases = (pkg.publicAliases ?? []).filter(matches);
    const defaultWitnesses = (pkg.defaultWitnesses ?? []).filter(matches);
    const owners = [...types, ...publicAliases, ...defaultWitnesses];
    const measured = new Set(owners.map(owner => owner.name));
    const locals = (pkg.locals ?? []).filter(local => owners.some(({span}) => {
      if (!span || path.resolve(workspace, span.filename) !== path.resolve(workspace, local.file)) return false;
      const column = local.column + 1;
      const afterStart = local.line > span.begin[0] || (local.line === span.begin[0] && column >= span.begin[1]);
      const beforeEnd = local.line < span.end[0] || (local.line === span.end[0] && column < span.end[1]);
      return afterStart && beforeEnd;
    }));
    const delegatedLocals = (pkg.delegatedLocals ?? []).filter(helper => measured.has(helper.runtimeOwner));
    return {...pkg, types, publicAliases, defaultWitnesses, locals, delegatedLocals};
  }).filter(pkg => pkg.types.length || pkg.publicAliases.length || pkg.defaultWitnesses.length);
}
