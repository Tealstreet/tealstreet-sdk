export const convertPxStrToInt = (pxStr: string) => {
  const rawStr = pxStr.slice(0, -2);
  return parseInt(rawStr);
};
