// Read artifact ZIPs without extracting paths or trusting an external unzip tool.
// Runtime restoration remains owned by cpu-runtime.mjs after archive validation.
import { inflateRawSync } from 'node:zlib';
import { parseCohortJson } from './cpu-cohort.mjs';

const fail = (message) => { throw new Error(`CPU artifact ZIP: ${message}`); };
const check = (condition, message) => { if (!condition) fail(message); };
const crcTable = Uint32Array.from({ length: 256 }, (_, value) => {
  for (let bit = 0; bit < 8; bit++) value = value & 1 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
  return value >>> 0;
});
const crc32 = (bytes) => {
  let crc = 0xffffffff;
  for (const byte of bytes) crc = crcTable[(crc ^ byte) & 255] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
};
function filename(bytes, flags) {
  check(flags & 0x800 || bytes.every((byte) => byte < 128), 'ambiguous filename encoding');
  let name;
  try { name = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes); }
  catch { fail('invalid UTF-8 filename'); }
  const directory = name.endsWith('/');
  if (directory) name = name.slice(0, -1);
  check(name && name === name.normalize('NFC') && !/[\\:\x00-\x1f\x7f]/.test(name) &&
    name.split('/').every((part) => part && part !== '.' && part !== '..'), 'unsafe or noncanonical filename');
  return { name, directory };
}
function extraFields(bytes) {
  const fields = new Map();
  for (let offset = 0; offset < bytes.length;) {
    check(offset + 4 <= bytes.length, 'truncated extra field');
    const id = bytes.readUInt16LE(offset), length = bytes.readUInt16LE(offset + 2);
    check(![0x7075, 0x6375, 0x9901, 0x756e, 0x000d].includes(id), 'unsupported alternate-name, encryption or Unix-link extra field');
    check(!fields.has(id), 'duplicate extra field');
    const start = offset + 4;
    offset += 4 + length;
    check(offset <= bytes.length, 'truncated extra field');
    fields.set(id, bytes.subarray(start, offset));
  }
  return fields;
}
function wide(bytes, offset, label) {
  check(offset >= 0 && offset + 8 <= bytes.length, `truncated ZIP64 ${label}`);
  const value = bytes.readBigUInt64LE(offset);
  check(value <= BigInt(Number.MAX_SAFE_INTEGER), `unsafe ZIP64 ${label}`);
  return Number(value);
}
// ZIP64 extra values appear only for sentinel fields, in this fixed order.
// Reject surplus data rather than accepting alternate interpretations.
function expanded(fields, { uncompressed, compressed, localOffset, disk }, needed) {
  const values = { uncompressed, compressed, localOffset, disk };
  const required = Object.entries(values).filter(([name, value]) => value !== undefined && value === (name === 'disk' ? 0xffff : 0xffffffff));
  const field = fields.get(1);
  if (!required.length) { check(!field, 'unexpected ZIP64 extra field'); return values; }
  check(needed === 45 && field, 'missing ZIP64 extra field/version');
  check(field.length === required.reduce((length, [name]) => length + (name === 'disk' ? 4 : 8), 0), 'ZIP64 extra field length mismatch');
  let offset = 0;
  for (const [name] of required) {
    values[name] = name === 'disk' ? field.readUInt32LE(offset) : wide(field, offset, name);
    offset += name === 'disk' ? 4 : 8;
  }
  return values;
}
function entryType(host, attributes, directory) {
  check(host === 0 || host === 3 || host === 19, 'unsupported entry platform');
  const dos = attributes & 0xffff, mode = attributes >>> 16, type = mode & 0o170000;
  check(!(dos & ~0x37) && !(mode & 0o7000), 'special entry attributes');
  check(type === 0 || type === (directory ? 0o040000 : 0o100000), 'symlink or special entry');
  check(!(dos & 0x10) || directory, 'conflicting directory attributes');
}

/** Validate the entire archive before returning regular file bytes. Directory
 * records are validated but omitted from the map; no filesystem writes occur. */
export function readArtifactZip(bytes, { maxBytes = 512 * 1024 * 1024, maxFiles = 200000 } = {}) {
  check(Buffer.isBuffer(bytes), 'archive must be a Buffer');
  check(Number.isSafeInteger(maxBytes) && maxBytes > 0 && Number.isSafeInteger(maxFiles) && maxFiles >= 0 && maxFiles <= 200000, 'invalid limits');
  check(bytes.length >= 22 && bytes.length <= maxBytes, 'archive size exceeds bounds or is truncated');
  const ends = [];
  for (let offset = Math.max(0, bytes.length - 22 - 65535); offset <= bytes.length - 22; offset++)
    if (bytes.readUInt32LE(offset) === 0x06054b50 && offset + 22 + bytes.readUInt16LE(offset + 20) === bytes.length) ends.push(offset);
  check(ends.length === 1, 'missing or ambiguous end record');
  const end = ends[0], diskCount = bytes.readUInt16LE(end + 8), legacyCount = bytes.readUInt16LE(end + 10);
  const legacySize = bytes.readUInt32LE(end + 12), legacyOffset = bytes.readUInt32LE(end + 16);
  check(bytes.readUInt16LE(end + 4) === 0 && bytes.readUInt16LE(end + 6) === 0, 'multi-disk archive');
  let count = legacyCount, centralSize = legacySize, centralOffset = legacyOffset, centralEnd = end;
  const locator = end - 20;
  if (locator >= 0 && bytes.readUInt32LE(locator) === 0x07064b50) {
    check(bytes.readUInt32LE(locator + 4) === 0 && bytes.readUInt32LE(locator + 16) === 1, 'split ZIP64 locator');
    const record = wide(bytes, locator + 8, 'end offset');
    check(record + 56 === locator && record >= 0 && bytes.readUInt32LE(record) === 0x06064b50, 'missing, overlapping or ambiguous ZIP64 end');
    check(wide(bytes, record + 4, 'end length') === 44 && bytes.readUInt16LE(record + 14) === 45, 'unsupported ZIP64 end length/version');
    check(bytes.readUInt32LE(record + 16) === 0 && bytes.readUInt32LE(record + 20) === 0, 'split ZIP64 end');
    count = wide(bytes, record + 32, 'entry count');
    check(wide(bytes, record + 24, 'disk entry count') === count, 'ZIP64 disk count mismatch');
    centralSize = wide(bytes, record + 40, 'central size'); centralOffset = wide(bytes, record + 48, 'central offset'); centralEnd = record;
    check([diskCount, legacyCount].every(value => value === 0xffff || value === count) &&
      (legacySize === 0xffffffff || legacySize === centralSize) && (legacyOffset === 0xffffffff || legacyOffset === centralOffset), 'ZIP64/classic end disagreement');
  } else check(diskCount === count && count !== 0xffff && centralSize !== 0xffffffff && centralOffset !== 0xffffffff, 'missing ZIP64 end or inconsistent counts');
  check(count <= maxFiles && centralSize <= maxBytes && centralOffset + centralSize === centralEnd, 'central directory bounds/count mismatch');
  let offset = centralOffset, compressedTotal = 0, uncompressedTotal = 0;
  const entries = [], names = new Map();
  for (let index = 0; index < count; index++) {
    check(offset + 46 <= centralEnd && bytes.readUInt32LE(offset) === 0x02014b50, 'truncated or invalid central header');
    const needed = bytes.readUInt16LE(offset + 6), flags = bytes.readUInt16LE(offset + 8), method = bytes.readUInt16LE(offset + 10);
    const crc = bytes.readUInt32LE(offset + 16);
    const nameLength = bytes.readUInt16LE(offset + 28), extraLength = bytes.readUInt16LE(offset + 30), commentLength = bytes.readUInt16LE(offset + 32);
    const next = offset + 46 + nameLength + extraLength + commentLength;
    check([10, 20, 45].includes(needed) && (method === 0 && !(flags & 8) || needed === 20 || needed === 45) &&
      [0, 8].includes(method) && !(flags & ~0x080e) && (method !== 0 || !(flags & 6)), 'unsupported version, compression or encrypted flags');
    check(next <= centralEnd, 'entry bounds exceed directory');
    const fields = extraFields(bytes.subarray(offset + 46 + nameLength, offset + 46 + nameLength + extraLength));
    const { compressed, uncompressed, localOffset, disk } = expanded(fields, {
      uncompressed: bytes.readUInt32LE(offset + 24), compressed: bytes.readUInt32LE(offset + 20),
      localOffset: bytes.readUInt32LE(offset + 42), disk: bytes.readUInt16LE(offset + 34),
    }, needed);
    check(disk === 0 && localOffset < centralOffset, 'split entry or local offset exceeds directory');
    const nameBytes = bytes.subarray(offset + 46, offset + 46 + nameLength);
    const { name, directory } = filename(nameBytes, flags);
    entryType(bytes[offset + 5], bytes.readUInt32LE(offset + 38), directory);
    check(!names.has(name), 'duplicate filename');
    names.set(name, directory);
    check(!directory || (method === 0 && compressed === 0 && uncompressed === 0 && crc === 0), 'directory contains data');
    compressedTotal += compressed; uncompressedTotal += uncompressed;
    check(compressedTotal <= maxBytes && uncompressedTotal <= maxBytes, 'compressed/uncompressed data exceeds limit');
    entries.push({ name, nameBytes, directory, needed, flags, method, crc, compressed, uncompressed, localOffset });
    offset = next;
  }
  check(offset === centralEnd, 'central directory size/count mismatch');
  for (const entry of entries) {
    const parts = entry.name.split('/');
    for (let i = 1; i < parts.length; i++) check(names.get(parts.slice(0, i).join('/')) !== false, 'file used as directory ancestor');
  }
  entries.sort((a, b) => a.localOffset - b.localOffset);
  check((entries[0]?.localOffset ?? centralOffset) === 0, 'unclaimed archive prefix');
  const result = new Map();
  for (let index = 0; index < entries.length; index++) {
    const entry = entries[index], start = entry.localOffset, boundary = entries[index + 1]?.localOffset ?? centralOffset;
    check(start + 30 <= boundary && bytes.readUInt32LE(start) === 0x04034b50, 'overlapping or invalid local header');
    check(bytes.readUInt16LE(start + 4) === entry.needed && bytes.readUInt16LE(start + 6) === entry.flags && bytes.readUInt16LE(start + 8) === entry.method, 'local/central header disagreement');
    const nameLength = bytes.readUInt16LE(start + 26), extraLength = bytes.readUInt16LE(start + 28);
    const payloadStart = start + 30 + nameLength + extraLength, payloadEnd = payloadStart + entry.compressed;
    check(payloadEnd <= boundary && bytes.subarray(start + 30, start + 30 + nameLength).equals(entry.nameBytes), 'local filename/data bounds disagreement');
    const localFields = extraFields(bytes.subarray(start + 30 + nameLength, payloadStart));
    const localSizes = expanded(localFields, { uncompressed: bytes.readUInt32LE(start + 22), compressed: bytes.readUInt32LE(start + 18) }, entry.needed);
    const local = [bytes.readUInt32LE(start + 14), localSizes.compressed, localSizes.uncompressed];
    const expected = [entry.crc, entry.compressed, entry.uncompressed];
    const matches = (values) => values.every((value, i) => value === expected[i]);
    if (entry.flags & 8) {
      check(matches(local) || local.every((value) => value === 0), 'local descriptor metadata disagreement');
      const length = boundary - payloadEnd;
      const wideSizes = length === 20 || length === 24;
      const signature = length === 16 || length === 24;
      check([12, 16, 20, 24].includes(length) && (!wideSizes || entry.needed === 45) &&
        (!signature || bytes.readUInt32LE(payloadEnd) === 0x08074b50), 'missing or ambiguous data descriptor');
      const at = payloadEnd + (signature ? 4 : 0);
      check(matches([bytes.readUInt32LE(at), wideSizes ? wide(bytes, at + 4, 'descriptor compressed size') : bytes.readUInt32LE(at + 4),
        wideSizes ? wide(bytes, at + 12, 'descriptor uncompressed size') : bytes.readUInt32LE(at + 8)]), 'data descriptor disagreement');
    } else check(matches(local) && payloadEnd === boundary, 'local size/CRC disagreement or unclaimed data');
    const compressed = bytes.subarray(payloadStart, payloadEnd);
    let output;
    if (entry.method === 0) {
      check(entry.compressed === entry.uncompressed, 'stored size disagreement');
      output = Buffer.from(compressed);
    } else {
      try {
        const inflated = inflateRawSync(compressed, { maxOutputLength: Math.max(1, entry.uncompressed), info: true });
        check(inflated.engine.bytesWritten === compressed.length, 'unclaimed deflate data');
        output = inflated.buffer;
      } catch (error) { fail(`invalid bounded deflate data: ${error.message}`); }
    }
    check(output.length === entry.uncompressed && crc32(output) === entry.crc, 'uncompressed size or CRC mismatch');
    if (!entry.directory) result.set(entry.name, output);
  }
  return result;
}

export function readArtifactJson(zipBytes, name) {
  check(typeof name === 'string' && filename(Buffer.from(name), 0x800).name === name && !name.endsWith('/'), 'invalid JSON member name');
  const bytes = readArtifactZip(zipBytes).get(name);
  check(bytes !== undefined, 'missing JSON member');
  return parseCohortJson(bytes);
}
