export const getApiPolicyUrl = (): string => {
  if (process.env.API_POLICY === 'same-domain') {
    return '';
  }
  return 'https://preview.tealstreet.io';
};
