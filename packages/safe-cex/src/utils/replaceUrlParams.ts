// replace params in url path if needed in form /{symbol}/ and remove from params
export const replaceUrlParams = (url: string | undefined, params: Record<string, string | number> = {}) => {
  if (!url) return { url, params };
  params = { ...params };
  const matches = url.match(/{\w+}/g);
  if (matches) {
    matches.forEach((match) => {
      const key = match.replace(/{|}/g, '');
      if (params[key]) {
        url = url!.replaceAll(match, params[key].toString());
        delete params[key];
      }
    });
  }
  return { url, params };
};
