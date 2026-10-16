const path = require('node:path');
const selected = process.env.KUCOIN_CARDINALITY_SCRIPT;
if (selected && path.basename(process.argv[1] || '') === selected) {
  const write = process.stdout.write.bind(process.stdout);
  process.stdout.write = (chunk, ...args) => {
    const capture = JSON.parse(String(chunk));
    const field = process.env.KUCOIN_CARDINALITY_PATH;
    const rows = field
      ? field.split('.').reduce((value, key) => value[key], capture)
      : Array.isArray(capture) ? capture : capture.cases;
    if (!Array.isArray(rows)) throw Error('cardinality control requires a captured case array');
    switch (process.env.KUCOIN_CARDINALITY_CAPTURE) {
      case 'first-null':
        if (!rows.length) throw Error('replacement requires a captured entry');
        rows[0] = null;
        break;
      case 'reverse': rows.reverse(); break;
      case 'replace':
        if (rows.length < 2) throw Error('replacement requires two captured entries');
        rows[0] = rows[1];
        break;
      case 'empty': rows.splice(0); break;
      case 'partial': rows.splice(1); break;
      case 'extra':
        if (!rows.length) throw Error('cardinality extra control requires a positive baseline');
        rows.push(rows[0]);
        break;
      default: throw Error('unknown cardinality capture control');
    }
    return write(JSON.stringify(capture) + '\n', ...args);
  };
}
