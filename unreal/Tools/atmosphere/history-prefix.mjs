// Select consumed atmosphere updates through one native tour screenshot request.
// This identifies the request's CPU state, not the GPU readback frame.
import assert from 'node:assert/strict';
import { basename, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { readFileSync, writeFileSync } from 'node:fs';
import { atmosphereHistoryRows } from './history-source-transform.mjs';
export function historyThroughRequest(log, tag) {
  assert.match(tag, /^[A-Za-z0-9_.-]+$/, 'one exact tour tag');
  const lines=log.split('\n'), matches=[];
  for(let i=0;i<lines.length;i++) {
    const m=lines[i].match(/tour:\s+(\S+\.png): target/);
    if(m&&basename(m[1])===tag+'.png')matches.push(i);
  }
  assert.equal(matches.length,1,'one exact native screenshot request is required');
  const rows=atmosphereHistoryRows(lines.slice(0,matches[0]).join('\n'));
  assert.ok(rows.length>1&&rows[0].event==='start','complete consumed history beginning at StartMatch is required');
  assert.equal(rows.filter(r=>r.event==='start').length,1,'multiple starts require their preceding source constructor history');
  assert.ok(Number.isSafeInteger(rows.at(-1).engineFrame),'native engineFrame correlation is required');
  return {rows,endpoint:{kind:'native-tour-request',tag,logLine:matches[0]+1,
    engineFrame:rows.at(-1).engineFrame,atmosphereFrame:rows.at(-1).frame,tick:rows.at(-1).tick,
    gpuReadbackCorrelated:false},limits:'CPU state through the request only. Verify the actual screenshot/render frame separately.'};
}
if(process.argv[1]&&import.meta.url===pathToFileURL(resolve(process.argv[1])).href) {
  const [file,tag,out]=process.argv.slice(2);assert.ok(file&&tag&&out,'native log, exact tour tag and output JSON are required');
  const selected=historyThroughRequest(readFileSync(file,'utf8'),tag);
  writeFileSync(out,JSON.stringify(selected,null,2)+'\n');
  console.log(JSON.stringify({frames:selected.rows.length-1,...selected.endpoint}));
}
