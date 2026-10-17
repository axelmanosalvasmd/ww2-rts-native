// node --import ./register.mjs: installs reference-hooks.mjs (three resolution and the internal-export append)
// as synchronous in-thread module hooks.
import { registerHooks } from 'node:module';
import { resolve, load } from './reference-hooks.mjs';
registerHooks({ resolve, load });
