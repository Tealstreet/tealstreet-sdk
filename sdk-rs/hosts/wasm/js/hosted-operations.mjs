/** One closed structured-operation channel. Family codecs retain all crypto policy. */
export function createOperationProvider() { return async () => { throw new Error('Operation refused'); }; }
export function createOperationPeer() { return { async handle() { throw new Error('Operation refused'); }, dispose() {} }; }
