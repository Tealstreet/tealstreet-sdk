/** Cache retirement only; neither this check nor retirement proves an order was not sent. */
export const canRetirePlacementReceipt = (result: unknown): boolean => {
  const ids: unknown = Array.isArray(result)
    ? result
    : result && typeof result === 'object' && 'orderIds' in result
      ? result.orderIds
      : undefined;
  if (!Array.isArray(ids)) return false;
  if (ids.length > 0) return ids.every((id) => typeof id === 'string' && id.length > 0);
  return 'protectionApplied' in ids && ids.protectionApplied === true;
};
