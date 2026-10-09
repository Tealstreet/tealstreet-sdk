// Imported for its side effect at the top of every Tealstreet entry point (web main
// thread and workers, CLI, exchange-server, candle-server, scripts). Without it the
// realm falls back to the direct transport and syncs its clock to venue time.
import { virtualClock } from '../utils/virtualClock';
import { setDefaultTransport } from './registry';
import { tealstreetTimeSource } from './tealstreet/tealstreetTimeSource';
import { tealstreetTransport } from './tealstreet/tealstreetTransport';

setDefaultTransport(tealstreetTransport);
virtualClock.setServerTimeSource(tealstreetTimeSource);
