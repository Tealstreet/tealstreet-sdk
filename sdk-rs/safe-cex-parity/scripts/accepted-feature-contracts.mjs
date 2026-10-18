import assert from 'node:assert/strict';

export function requiredFeatures(contract, name) {
  if (contract?.requiredFeatures === undefined) return [];
  const features = contract.requiredFeatures;
  assert.ok(Array.isArray(features) && features.length && features.every(feature =>
    typeof feature === 'string' && feature.length && feature.trim() === feature),
  `${name}: requiredFeatures must be a nonempty array of feature names`);
  assert.equal(new Set(features).size, features.length, `${name}: duplicate required feature`);
  assert.ok(contract.category !== 'non-decoder' && contract.shape !== 'typed',
    `${name}: required features need executed decoder observations`);
  return features;
}

export function featurePlan(packages, contracts, metadata, configuration) {
  const groups = new Map();
  const gated = [];
  const ordinary = packages.map(pkg => {
    const declared = metadata.packages.find(item => item.id === pkg.packageId);
    assert.ok(declared, `${pkg.package}: feature contract needs compiler package metadata`);
    const deferred = new Set();
    const select = owner => {
      const required = requiredFeatures(contracts?.[owner.name], owner.name);
      assert.ok(!required.length || pkg.types.includes(owner), `${owner.name}: feature requirements need a decoder owner API`);
      for (const feature of required) assert.ok(Object.hasOwn(declared.features, feature),
        `${owner.name}: required feature is not declared by its Cargo package: ${feature}`);
      if (!required.length || (configuration.target === 'native' && required.every(feature => pkg.features.includes(feature)))) return true;
      const features = [...new Set([...pkg.features, ...required])].sort();
      const key = JSON.stringify([pkg.package, features]);
      const group = groups.get(key) ?? { package: pkg.package, features, owners: [] };
      group.owners.push(owner.name);
      groups.set(key, group);
      deferred.add(owner.name);
      gated.push({name: owner.name, classificationOnly: true, category: 'native-gated',
        status: 'native-gated', requiredFeatures: required, observed: false,
        configuration, compilerFeatures: pkg.features, failures: []});
      return false;
    };
    const types = pkg.types.filter(select);
    const publicAliases = (pkg.publicAliases ?? []).filter(select);
    const defaultWitnesses = (pkg.defaultWitnesses ?? []).filter(select);
    return {...pkg, types, publicAliases, defaultWitnesses,
      delegatedLocals: (pkg.delegatedLocals ?? []).filter(helper => !deferred.has(helper.runtimeOwner))};
  });
  return {ordinary, gated, groups: [...groups.values()]};
}

export function requireFeatureReceipts(group, results) {
  for (const name of group.owners) {
    const receipts = results.filter(row => row.name === name && !row.classificationOnly
      && row.configuration.target === 'native'
      && group.features.every(feature => row.compilerFeatures?.includes(feature)));
    assert.ok(receipts.length, `${name}: required native feature observation did not execute`);
  }
}
