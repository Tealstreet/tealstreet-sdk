export const getApiPolicy = (): 'same-domain' | 'production' => {
  if (process.env.API_POLICY === 'same-domain') {
    return 'same-domain';
  }
  return 'production';
};
