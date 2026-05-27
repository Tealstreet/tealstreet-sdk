export function getPrecision(num: number | string) {
  const val = Number(num);
  if (isNaN(val)) {
    return 0;
  }

  const strList = String(num).split('.');
  return strList.length === 2 ? strList[1].length : 0;
}

export function generateApeXRandomClientId() {
  return 'apexomni-tealstreet-' + Math.random().toString().slice(2).replace(/^0+/, '');
}

export function dropPrefix(hash: string) {
  return hash.replace(/^0x/, '');
}
