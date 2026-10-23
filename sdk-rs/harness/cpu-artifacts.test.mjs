import assert from 'node:assert/strict';
import { test } from 'node:test';
import { crc32, deflateRawSync } from 'node:zlib';
import { readArtifactJson, readArtifactZip } from './cpu-artifacts.mjs';

// Independent ZIP writer only for bounded fake fixtures; production only reads.
function zip(members, { gap = Buffer.alloc(0), comment = Buffer.alloc(0), zip64 = false } = {}) {
  const locals = [], central = [];
  let offset = 0;
  for (const member of members) {
    const name = Buffer.isBuffer(member.name) ? member.name : Buffer.from(member.name);
    const data = Buffer.from(member.data ?? ''), method = member.method ?? 0;
    const compressed = member.compressed ?? (method === 8 ? deflateRawSync(data) : data);
    const extra = member.extra ?? Buffer.alloc(0), flags = (member.flags ?? 0x800) | (member.descriptor ? 8 : 0);
    const extended = (values, disk = false) => {
      const field = Buffer.alloc(4 + values.length * 8 + (disk ? 4 : 0));
      field.writeUInt16LE(1); field.writeUInt16LE(field.length - 4, 2);
      values.forEach((value, index) => field.writeBigUInt64LE(BigInt(value), 4 + index * 8));
      return field;
    };
    const version = member.zip64Sizes || member.zip64Offset || member.zip64Disk ? 45 : 20;
    const localExtra = Buffer.concat([extra, ...(member.zip64Sizes ? [extended(member.descriptor ? [0, 0] : [data.length, compressed.length])] : [])]);
    const centralExtra = Buffer.concat([extra, ...(version === 45 ? [extended([
      ...(member.zip64Sizes ? [data.length, compressed.length] : []), ...(member.zip64Offset ? [offset] : []),
    ], member.zip64Disk)] : [])]);
    const checksum = crc32(data), local = Buffer.alloc(30), header = Buffer.alloc(46);
    local.writeUInt32LE(0x04034b50); local.writeUInt16LE(version, 4); local.writeUInt16LE(flags, 6); local.writeUInt16LE(method, 8);
    if (!member.descriptor) { local.writeUInt32LE(checksum, 14); local.writeUInt32LE(compressed.length, 18); local.writeUInt32LE(data.length, 22); }
    if (member.zip64Sizes) { local.writeUInt32LE(0xffffffff, 18); local.writeUInt32LE(0xffffffff, 22); }
    local.writeUInt16LE(name.length, 26); local.writeUInt16LE(localExtra.length, 28);
    const descriptor = Buffer.alloc(member.descriptor ?? 0);
    if (member.descriptor) {
      const start = [16, 24].includes(member.descriptor) ? 4 : 0;
      if (start) descriptor.writeUInt32LE(0x08074b50);
      descriptor.writeUInt32LE(checksum, start);
      if ([20, 24].includes(member.descriptor)) { descriptor.writeBigUInt64LE(BigInt(compressed.length), start + 4); descriptor.writeBigUInt64LE(BigInt(data.length), start + 12); }
      else { descriptor.writeUInt32LE(compressed.length, start + 4); descriptor.writeUInt32LE(data.length, start + 8); }
    }
    header.writeUInt32LE(0x02014b50); header.writeUInt16LE((3 << 8) | version, 4); header.writeUInt16LE(version, 6);
    header.writeUInt16LE(flags, 8); header.writeUInt16LE(method, 10); header.writeUInt32LE(checksum, 16);
    header.writeUInt32LE(compressed.length, 20); header.writeUInt32LE(data.length, 24);
    if (member.zip64Sizes) { header.writeUInt32LE(0xffffffff, 20); header.writeUInt32LE(0xffffffff, 24); }
    if (member.zip64Disk) header.writeUInt16LE(0xffff, 34);
    header.writeUInt16LE(name.length, 28); header.writeUInt16LE(centralExtra.length, 30);
    header.writeUInt32LE(member.attributes ?? (((member.name.toString().endsWith('/') ? 0o040755 : 0o100644) << 16) >>> 0), 38);
    header.writeUInt32LE(member.zip64Offset ? 0xffffffff : offset, 42);
    const record = Buffer.concat([local, name, localExtra, compressed, descriptor, gap]);
    locals.push(record); central.push(header, name, centralExtra); offset += record.length;
  }
  const directory = Buffer.concat(central), end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50); end.writeUInt16LE(zip64 ? 0xffff : members.length, 8); end.writeUInt16LE(zip64 ? 0xffff : members.length, 10);
  end.writeUInt32LE(zip64 ? 0xffffffff : directory.length, 12); end.writeUInt32LE(zip64 ? 0xffffffff : offset, 16); end.writeUInt16LE(comment.length, 20);
  const extendedEnd = [];
  if (zip64) {
    const record = Buffer.alloc(56), locator = Buffer.alloc(20);
    record.writeUInt32LE(0x06064b50); record.writeBigUInt64LE(44n, 4); record.writeUInt16LE(45, 12); record.writeUInt16LE(45, 14);
    record.writeBigUInt64LE(BigInt(members.length), 24); record.writeBigUInt64LE(BigInt(members.length), 32);
    record.writeBigUInt64LE(BigInt(directory.length), 40); record.writeBigUInt64LE(BigInt(offset), 48);
    locator.writeUInt32LE(0x07064b50); locator.writeBigUInt64LE(BigInt(offset + directory.length), 8); locator.writeUInt32LE(1, 16);
    extendedEnd.push(record, locator);
  }
  return Buffer.concat([...locals, directory, ...extendedEnd, end, comment]);
}
const end = (bytes) => bytes.length - 22;
const central = (bytes) => bytes.readUInt32LE(end(bytes) + 16);
const changed = (bytes, mutate) => { const result = Buffer.from(bytes); mutate(result); return result; };
const sample = () => zip([{ name: 'case/report.json', data: '{"schema":1}' }, { name: 'raw/out.json', data: '{"ok":true}', method: 8 }]);

test('stored/deflated nested capsule and JSON entries retain exact bytes', () => {
  const bytes = zip([
    { name: 'runtime/', data: '' },
    { name: 'runtime/manifest.json', data: '{"schema":1,"entries":[]}', method: 8 },
    { name: 'runtime/blobs/abcdef', data: Buffer.from([0, 1, 255, 10]) },
    { name: 'empty', method: 8 },
    { name: 'résultat.json', data: '{"case":{"venue":"mexc"}}', method: 8 },
  ], { comment: Buffer.from('artifact comment') });
  const files = readArtifactZip(bytes);
  assert.deepEqual([...files.keys()], ['runtime/manifest.json', 'runtime/blobs/abcdef', 'empty', 'résultat.json']);
  assert.deepEqual(files.get('runtime/blobs/abcdef'), Buffer.from([0, 1, 255, 10]));
  assert.deepEqual(readArtifactJson(bytes, 'runtime/manifest.json'), { schema: 1, entries: [] });
  assert.equal(readArtifactJson(bytes, 'résultat.json').case.venue, 'mexc');
  assert.equal(readArtifactZip(zip([])).size, 0);
});

test('both descriptor encodings are validated against central metadata', () => {
  for (const descriptor of [12, 16]) {
    const bytes = zip([{ name: 'report.json', data: '{"complete":true}', method: 8, descriptor }]);
    assert.deepEqual(readArtifactJson(bytes, 'report.json'), { complete: true });
    assert.throws(() => readArtifactZip(changed(bytes, (value) => { value[central(value) - 1] ^= 1; })), /descriptor/);
  }
});

test('CRC, stored size and exact deflate consumption reject damaged data', () => {
  const stored = zip([{ name: 'raw', data: '123456789' }]);
  assert.equal(stored.readUInt32LE(14), 0xcbf43926, 'standard CRC32 vector is independent of reader');
  assert.throws(() => readArtifactZip(changed(stored, (bytes) => { bytes[33] ^= 1; })), /CRC/);
  assert.throws(() => readArtifactZip(changed(stored, (bytes) => { bytes.writeUInt32LE(8, 22); bytes.writeUInt32LE(8, central(bytes) + 24); })), /stored size/);
  const compressed = deflateRawSync(Buffer.from('complete'));
  assert.throws(() => readArtifactZip(zip([{ name: 'raw', data: 'complete', method: 8, compressed: Buffer.concat([compressed, Buffer.from('trailing')]) }])), /unclaimed deflate/);
  assert.throws(() => readArtifactZip(zip([{ name: 'raw', data: 'complete', method: 8, compressed: Buffer.from('invalid') }])), /deflate/);
});

test('unsafe, duplicate, alternate and ambiguous names fail before returning entries', () => {
  for (const name of ['../outside', '/absolute', 'a/../b', 'a//b', './relative', 'C:/drive', 'back\\slash', 'a\0b', 'a\nb', 'e\u0301.json'])
    assert.throws(() => readArtifactZip(zip([{ name }])), /filename/);
  for (const members of [
    [{ name: 'same' }, { name: 'same' }],
    [{ name: 'same/' }, { name: 'same' }],
    [{ name: 'file/child' }, { name: 'file' }],
  ]) assert.throws(() => readArtifactZip(zip(members)), /duplicate|ancestor/);
  assert.throws(() => readArtifactZip(zip([{ name: 'é', flags: 0 }])), /encoding/);
  assert.throws(() => readArtifactZip(zip([{ name: Buffer.from([0xff]), flags: 0x800 }])), /UTF-8/);
  for (const id of [0x0001, 0x7075, 0x6375, 0x756e, 0x000d]) {
    const extra = Buffer.alloc(4); extra.writeUInt16LE(id);
    assert.throws(() => readArtifactZip(zip([{ name: 'raw', extra }])), /extra field/);
  }
});

test('symlinks, special files, privileged modes and encrypted methods are refused', () => {
  for (const mode of [0o120777, 0o010644, 0o140644, 0o104755])
    assert.throws(() => readArtifactZip(zip([{ name: 'raw', attributes: (mode << 16) >>> 0 }])), /special|attributes/);
  for (const attributes of [8, 0x40, 0x400])
    assert.throws(() => readArtifactZip(zip([{ name: 'raw', attributes }])), /attributes/);
  for (const flags of [1, 0x40, 0x2000, 0x10])
    assert.throws(() => readArtifactZip(zip([{ name: 'raw', flags }])), /flags/);
  assert.throws(() => readArtifactZip(zip([{ name: 'raw', method: 9 }])), /compression/);
  assert.throws(() => readArtifactZip(zip([{ name: 'directory/', data: 'hidden' }])), /directory contains data/);
});

test('central/end counts, offsets, truncation, trailing bytes and overlapping records fail closed', () => {
  const bytes = sample();
  for (const mutate of [
    (value) => value.writeUInt16LE(1, end(value) + 4),
    (value) => value.writeUInt16LE(1, end(value) + 8),
    (value) => value.writeUInt16LE(0xffff, end(value) + 8),
    (value) => { value.writeUInt16LE(0xffff, end(value) + 8); value.writeUInt16LE(0xffff, end(value) + 10); },
    (value) => value.writeUInt32LE(0xffffffff, end(value) + 12),
    (value) => value.writeUInt32LE(central(value) + 1, end(value) + 16),
    (value) => value.writeUInt16LE(2, central(value) + 34),
    (value) => value.writeUInt32LE(0xffffffff, central(value) + 20),
    (value) => value.writeUInt16LE(45, central(value) + 6),
    (value) => { const second = central(value) + 46 + value.readUInt16LE(central(value) + 28); value.writeUInt32LE(0, second + 42); },
  ]) assert.throws(() => readArtifactZip(changed(bytes, mutate)));
  for (const length of [0, 4, 21, 22, bytes.length - 1, central(bytes) + 10])
    assert.throws(() => readArtifactZip(bytes.subarray(0, length)));
  assert.throws(() => readArtifactZip(Buffer.concat([bytes, Buffer.from('unclaimed')])));
  assert.throws(() => readArtifactZip(zip([{ name: 'raw', data: 'a' }], { gap: Buffer.from('gap') })), /unclaimed data/);
});

test('local names, flags, sizes, CRC, extra fields and methods must agree', () => {
  const bytes = sample();
  for (const mutate of [
    (value) => { value[30] ^= 1; },
    (value) => value.writeUInt16LE(0, 6),
    (value) => value.writeUInt16LE(8, 8),
    (value) => value.writeUInt32LE(0, 14),
    (value) => value.writeUInt32LE(0, 18),
    (value) => value.writeUInt16LE(65535, 26),
    (value) => value.writeUInt16LE(65535, 28),
  ]) assert.throws(() => readArtifactZip(changed(bytes, mutate)), /local|bounds/);
});

test('total expansion, archive bytes, file count and caller limits are bounded', () => {
  const bytes = sample();
  assert.throws(() => readArtifactZip(bytes, { maxFiles: 1 }), /count/);
  assert.throws(() => readArtifactZip(bytes, { maxBytes: bytes.length - 1 }), /size/);
  const bomb = zip([{ name: 'large', data: 'a'.repeat(100000), method: 8 }]);
  assert.throws(() => readArtifactZip(bomb, { maxBytes: 1000 }), /uncompressed/);
  for (const options of [{ maxBytes: 0 }, { maxBytes: Infinity }, { maxFiles: -1 }, { maxFiles: 200001 }, { maxFiles: 1.5 }])
    assert.throws(() => readArtifactZip(bytes, options), /limits/);
  assert.throws(() => readArtifactZip(new Uint8Array(bytes)), /Buffer/);
});

test('bounded ZIP64 end records transport more than 65535 content-addressed blobs', () => {
  const members = Array.from({ length: 65536 }, (_, index) => ({ name: `blobs/${index.toString(16).padStart(8, '0')}`, data: '' }));
  const bytes = zip(members, { zip64: true });
  // Streaming uploaders need only extend the count for a sub-4GiB archive.
  const record = end(bytes) - 20 - 56;
  bytes.writeUInt32LE(Number(bytes.readBigUInt64LE(record + 40)), end(bytes) + 12);
  bytes.writeUInt32LE(Number(bytes.readBigUInt64LE(record + 48)), end(bytes) + 16);
  const files = readArtifactZip(bytes);
  assert.equal(files.size, 65536);
  assert.deepEqual(files.get('blobs/0000ffff'), Buffer.alloc(0));
  assert.throws(() => readArtifactZip(bytes, { maxFiles: 65535 }), /count/);
});

test('ZIP64 size, local offset, disk zero and both wide descriptor encodings retain exact bytes', () => {
  for (const descriptor of [undefined, 20, 24]) {
    const bytes = zip([{ name: 'raw.json', data: '{"wide":true}', method: 8, zip64Sizes: true, zip64Offset: true, zip64Disk: true, descriptor }], { zip64: true });
    assert.deepEqual(readArtifactJson(bytes, 'raw.json'), { wide: true });
  }
  // Count-only extension leaves ordinary file headers and descriptors intact.
  assert.equal(readArtifactZip(zip([{ name: 'empty', descriptor: 12 }], { zip64: true })).size, 1);
});

test('ZIP64 locators/end records reject truncation, unsafe integers, split disks and conflicting metadata', () => {
  const bytes = zip([{ name: 'raw', data: 'strict' }], { zip64: true });
  const locator = end(bytes) - 20, record = locator - 56;
  for (const mutate of [
    value => value.writeUInt32LE(0, locator),
    value => value.writeUInt32LE(1, locator + 4),
    value => value.writeUInt32LE(2, locator + 16),
    value => value.writeBigUInt64LE(2n ** 63n, locator + 8),
    value => value.writeBigUInt64LE(BigInt(record + 1), locator + 8),
    value => value.writeBigUInt64LE(45n, record + 4),
    value => value.writeUInt32LE(1, record + 16),
    value => value.writeBigUInt64LE(2n, record + 24),
    value => value.writeBigUInt64LE(200001n, record + 32),
    value => value.writeBigUInt64LE(2n ** 63n, record + 40),
    value => value.writeBigUInt64LE(1n, record + 48),
    value => value.writeUInt16LE(2, end(value) + 10),
    value => value.writeUInt32LE(1, end(value) + 12),
  ]) assert.throws(() => readArtifactZip(changed(bytes, mutate)));
  for (const length of [record + 4, record + 55, locator + 4, locator + 19, bytes.length - 1]) assert.throws(() => readArtifactZip(bytes.subarray(0, length)));
  // A second end record cannot hide in the claimed central/local byte range.
  const duplicate = Buffer.concat([bytes.subarray(0, record), bytes.subarray(record, record + 56), bytes.subarray(record)]);
  duplicate.writeBigUInt64LE(BigInt(record + 56), end(duplicate) - 20 + 8);
  assert.throws(() => readArtifactZip(duplicate), /bounds|count|ambiguous/);
});

test('ZIP64 entry extensions and wide descriptors refuse missing, extra, huge and mismatched fields', () => {
  const bytes = zip([{ name: 'raw', data: 'strict', method: 8, zip64Sizes: true, zip64Offset: true, descriptor: 24 }], { zip64: true });
  const record = end(bytes) - 20 - 56, directory = Number(bytes.readBigUInt64LE(record + 48));
  const extra = directory + 46 + 3;
  for (const mutate of [
    value => value.writeUInt16LE(2, extra),
    value => value.writeUInt16LE(23, extra + 2),
    value => value.writeBigUInt64LE(2n ** 63n, extra + 4),
    value => value.writeBigUInt64LE(1n, extra + 12),
    value => value.writeBigUInt64LE(1n, extra + 20),
    value => value.writeUInt16LE(20, directory + 6),
    value => value.writeUInt16LE(0, 30 + 3),
    value => value.writeBigUInt64LE(1n, directory - 16),
  ]) assert.throws(() => readArtifactZip(changed(bytes, mutate)));
});

test('JSON members use strict duplicate-key, finite-number and UTF-8 validation', () => {
  for (const data of ['{"a":1,"a":2}', '{"n":1e999}', Buffer.from([0xff]), '{"a":1} trailing'])
    assert.throws(() => readArtifactJson(zip([{ name: 'report.json', data }]), 'report.json'));
  assert.throws(() => readArtifactJson(sample(), 'missing.json'), /missing JSON/);
  assert.throws(() => readArtifactJson(sample(), '../report.json'), /filename|member/);
  const damaged = sample(); damaged[30] ^= 1;
  assert.throws(() => readArtifactJson(damaged, 'raw/out.json'), /local filename/, 'the whole archive is checked, not only the requested member');
});
