// node --import ./register.mjs: the module hooks of unreal/Tools/models/infantry (bare 'three' from INFANTRY_DEPS,
// and the infantry modules' internals exported in memory, the files on disk untouched), plus the inert browser shims
// of unreal/Tools/models/shims.mjs, so the pinned reference client modules (unit-models.js, squad-motion.js,
// model-motion.js, wheel-motion.js, track-motion.js, aircraft.js) load under Node unmodified.
import { registerHooks } from 'node:module';
import { resolve, load } from '../models/infantry/reference-hooks.mjs';
import { installShims } from '../models/shims.mjs';
registerHooks({ resolve, load });
installShims();
