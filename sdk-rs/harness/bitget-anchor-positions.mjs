import { SECTION } from '../hosts/wasm/js/wire.mjs';

export function replayPositions(frame, venue, privateTape) {
  // The frozen cumulative anchor emits revision 1 even for an empty public
  // account. Its canonical zero-row payload has exactly the same meaning as
  // revision 2. No historical row is decoded, and production/private readers
  // retain their revision-2 requirement. Keep the original anchor CPU reference.
  if (venue === 'bitget' && privateTape === false) {
    const section = frame.first(SECTION.POSITIONS);
    if (section?.revision === 1 && section.length === 8 && frame.view.getUint32(section.offset, true) === 0 &&
        frame.view.getUint32(section.offset + 4, true) === 0) return { count: 0 };
  }
  return frame.positions();
}
