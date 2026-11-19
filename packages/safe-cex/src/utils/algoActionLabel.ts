/** Display names only; callers still dispatch the original operation unchanged. */
export const algoActionLabel = (op: string): string => {
  switch (op) {
    case 'chaser:start': return 'Starting the chase';
    case 'chaser:terminate': return 'Stopping the chase';
    case 'twap:start': return 'Starting the TWAP';
    case 'twap:resume': return 'Resuming the TWAP';
    case 'twap:pause': return 'Pausing the TWAP';
    case 'twap:stop': return 'Stopping the TWAP';
    case 'exec': return 'Running this trading request';
    default: return 'This automated order request';
  }
};
