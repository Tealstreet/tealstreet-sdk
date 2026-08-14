import { describe, expect, it } from 'vitest';

import { validateTargets, type RuntimeDescriptor, type ScriptDescriptor } from './targetCheck';

const standaloneRuntime: RuntimeDescriptor = {
  mountedTargets: ['core', 'cli'],
  coreApiVersion: '1.0.0',
  cliApiVersion: '1.0.0',
};

const webRuntime: RuntimeDescriptor = {
  mountedTargets: ['core', 'web'],
  coreApiVersion: '1.0.0',
  webApiVersion: '1.0.0',
};

describe('validateTargets (SPEC §2.5)', () => {
  it('accepts a core-only script on standalone runtime', () => {
    const script: ScriptDescriptor = { targets: ['core'], coreApiVersion: '1.0.0' };
    expect(() => validateTargets(script, standaloneRuntime)).not.toThrow();
  });

  it('rejects a web-only script on standalone runtime', () => {
    const script: ScriptDescriptor = {
      targets: ['core', 'web'],
      coreApiVersion: '1.0.0',
      webApiVersion: '1.0.0',
    };
    expect(() => validateTargets(script, standaloneRuntime)).toThrow(
      "this script targets 'web'; this runtime supports 'cli'",
    );
  });

  it('accepts a cross-target script when the current runtime is included', () => {
    const script: ScriptDescriptor = {
      targets: ['core', 'web', 'mobile', 'cli'],
      coreApiVersion: '1.0.0',
      webApiVersion: '9.0.0',
      mobileApiVersion: '9.0.0',
      cliApiVersion: '1.0.0',
    };
    expect(() => validateTargets(script, standaloneRuntime)).not.toThrow();
  });

  it('accepts a web-target script on web runtime', () => {
    const script: ScriptDescriptor = {
      targets: ['core', 'web'],
      coreApiVersion: '1.0.0',
      webApiVersion: '1.0.0',
    };
    expect(() => validateTargets(script, webRuntime)).not.toThrow();
  });

  it('rejects when runtime is older than the script requires', () => {
    const script: ScriptDescriptor = { targets: ['core'], coreApiVersion: '2.0.0' };
    expect(() => validateTargets(script, standaloneRuntime)).toThrow(
      'script requires CORE v2.0.0; runtime provides v1.0.0',
    );
  });

  it('treats missing script version as 0.0.0 (accepts any non-empty runtime)', () => {
    const script: ScriptDescriptor = { targets: ['core'] };
    expect(() => validateTargets(script, standaloneRuntime)).not.toThrow();
  });

  it('rejects when runtime lacks a version for its matched declared target', () => {
    const script: ScriptDescriptor = {
      targets: ['core', 'web'],
      coreApiVersion: '1.0.0',
      webApiVersion: '1.0.0',
    };
    const runtimeNoWebVersion: RuntimeDescriptor = {
      mountedTargets: ['core', 'web'],
      coreApiVersion: '1.0.0',
      // webApiVersion omitted on purpose
    };
    expect(() => validateTargets(script, runtimeNoWebVersion)).toThrow(/runtime provides no 'web' API version/);
  });

  it('does not require versions for non-current declared runtimes', () => {
    const script: ScriptDescriptor = {
      targets: ['core', 'web', 'mobile', 'cli'],
      coreApiVersion: '1.0.0',
      webApiVersion: '9.0.0',
      mobileApiVersion: '9.0.0',
      cliApiVersion: '1.0.0',
    };
    expect(() => validateTargets(script, standaloneRuntime)).not.toThrow();
  });

  it('handles patch-level version comparisons correctly', () => {
    const runtime: RuntimeDescriptor = {
      mountedTargets: ['core'],
      coreApiVersion: '1.0.5',
    };
    expect(() => validateTargets({ targets: ['core'], coreApiVersion: '1.0.3' }, runtime)).not.toThrow();
    expect(() => validateTargets({ targets: ['core'], coreApiVersion: '1.0.6' }, runtime)).toThrow(
      'script requires CORE v1.0.6; runtime provides v1.0.5',
    );
  });
});
