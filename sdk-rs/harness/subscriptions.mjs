const add = (bucket, value) => {
  if (!bucket) return value;
  const rows = Array.isArray(bucket) ? bucket : [bucket];
  const at = rows.findIndex((row) => row.order > value.order);
  if (at < 0) rows.push(value);
  else rows.splice(at, 0, value);
  return rows;
};
const remove = (bucket, value) => {
  if (!Array.isArray(bucket)) return bucket === value ? undefined : bucket;
  bucket.splice(bucket.indexOf(value), 1);
  return bucket.length === 1 ? bucket[0] : bucket;
};
const EMPTY = [];
const rows = (bucket) => !bucket ? EMPTY : Array.isArray(bucket) ? bucket : [bucket];
const coveredBy = (topic, arg, object) => Object.entries(topic).every(([key, value]) => {
  if (key === 'channel') return true;
  if (key === 'instType' && value === 'ANY') return true;
  const values = [arg[key], object[key]].filter((v) => v !== undefined && v !== '');
  if (!values.length && key === 'instType') {
    const id = object.instId ?? arg.instId;
    if (id?.endsWith('-SWAP')) values.push('SWAP');
  }
  return values.length > 0 && values.every((v) => v === value);
});

/** Index candidates; retain the complete subscription predicate below. */
export class SocketSubscriptions {
  topics = new Map();
  channels = new Map();
  nextOrder = 0;
  set(key, topic) {
    const old = this.topics.get(key);
    const order = old?.order ?? this.nextOrder++;
    if (old) this.unindex(old);
    let channel = this.channels.get(topic.channel);
    if (!channel) this.channels.set(topic.channel, channel = { ids: new Map(), wildcard: undefined });
    const hasId = Object.prototype.propertyIsEnumerable.call(topic, 'instId');
    const value = { topic, order, hasId };
    if (hasId) channel.ids.set(topic.instId, add(channel.ids.get(topic.instId), value));
    else channel.wildcard = add(channel.wildcard, value);
    this.topics.set(key, value);
  }
  delete(key) {
    const old = this.topics.get(key);
    if (!old) return false;
    this.unindex(old);
    this.topics.delete(key);
    return true;
  }
  unindex(old) {
    const channel = this.channels.get(old.topic.channel);
    if (old.hasId) {
      const bucket = remove(channel.ids.get(old.topic.instId), old);
      if (bucket) channel.ids.set(old.topic.instId, bucket);
      else channel.ids.delete(old.topic.instId);
    } else channel.wildcard = remove(channel.wildcard, old);
    if (channel.ids.size === 0 && !channel.wildcard) this.channels.delete(old.topic.channel);
  }
  assertSubscribed(socket, message) {
    if (!message.arg?.channel || !Array.isArray(message.data)) return;
    const channel = this.channels.get(message.arg.channel);
    const messages = message.data.length ? message.data : [null];
    for (const row of messages) {
      const object = row && !Array.isArray(row) && typeof row === 'object' ? row : {};
      const ids = [message.arg.instId, object.instId].filter((v) => v !== undefined && v !== '');
      let covered = false;
      if (ids.some((id) => id !== null && typeof id !== 'string')) {
        // Preserve legacy strict comparison and error ordering for malformed
        // non-string IDs, including an instType inference TypeError.
        for (const { topic } of this.topics.values()) {
          if (topic.channel === message.arg.channel && coveredBy(topic, message.arg, object)) {
            covered = true;
            break;
          }
        }
      } else {
        const id = ids[0];
        const exact = rows(ids.length && ids.every((value) => value === id) ? channel?.ids.get(id) : undefined);
        const wildcard = rows(channel?.wildcard);
        // Merge in original Map insertion order, preserving predicate error
        // ordering and duplicate replacement semantics without a sort/copy.
        let a = 0;
        let b = 0;
        while (a < exact.length || b < wildcard.length) {
          const next = b === wildcard.length || (a < exact.length && exact[a].order < wildcard[b].order)
            ? exact[a++] : wildcard[b++];
          if (coveredBy(next.topic, message.arg, object)) {
            covered = true;
            break;
          }
        }
      }
      if (!covered) throw new Error(`Unsubscribed ${message.arg.channel} frame on ${socket.url}: ${JSON.stringify(message.arg)}${object.instId ? ` instId=${object.instId}` : ''}`);
    }
  }
}
