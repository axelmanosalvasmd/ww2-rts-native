// node --import ./register.mjs: resolves the bare 'three' specifier of the read-only reference sources from the
// directory in WW2_MODEL_DEPS (see three-resolve.mjs).
import { registerHooks } from 'node:module';
import { resolveThree } from './three-resolve.mjs';
registerHooks({ resolve: resolveThree });
