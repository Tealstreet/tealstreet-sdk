export const isNodeOrBunRuntime = (): boolean =>
  typeof process !== 'undefined' && (Boolean(process.versions?.node) || Boolean((process as any).isBun));
