// Original MOV evidence only. No Source execution occurs when this module is imported.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {closeSync, createReadStream, openSync, readSync, renameSync, unlinkSync, writeSync} from 'node:fs';
import {pipeline} from 'node:stream/promises';
import {StringDecoder} from 'node:string_decoder';
import {createGunzip, gzipSync} from 'node:zlib';

export const format = 'mov-original-jsonl-gzip-v1';

// Each line is one complete gzip member. Compression boundaries follow record
// boundaries, never asynchronous stream chunking or wall time.
export function captureWriter(path) {
  const fd = openSync(path, 'wx');
  let sequence = 0, closed = false;
  return {
    write(record) {
      assert.equal(closed, false);
      const bytes = gzipSync(JSON.stringify({sequence: sequence++, ...record}) + '\n', {level: 9});
      for (let offset = 0; offset < bytes.length;) {
        const written = writeSync(fd, bytes, offset, bytes.length - offset);
        assert.ok(written > 0, 'capture write advances');
        offset += written;
      }
    },
    close() { if (!closed) { closed = true; closeSync(fd); } },
  };
}

function equalFiles(left, right) {
  const a = openSync(left, 'r');
  let b;
  try {
    b = openSync(right, 'r');
    const x = Buffer.alloc(65536), y = Buffer.alloc(65536);
    // Positional reads compare equal offsets even if a filesystem read is short.
    let position = 0;
    while (true) {
      const n = readSync(a, x, 0, x.length, position);
      let m = 0;
      while (m < n) {
        const count = readSync(b, y, m, n - m, position + m);
        if (count === 0) return false;
        m += count;
      }
      if (!x.subarray(0, n).equals(y.subarray(0, m))) return false;
      if (n === 0) return readSync(b, y, 0, 1, position) === 0;
      position += n;
    }
  } finally { closeSync(a); if (b !== undefined) closeSync(b); }
}

export function finishCapture(partial, output, checking, failed) {
  if (failed) renameSync(partial, checking ? `${output}.failed.gz` : output);
  else if (!checking) renameSync(partial, output);
  else {
    if (equalFiles(partial, output)) unlinkSync(partial);
    else {
      renameSync(partial, `${output}.different.gz`);
      assert.fail('original full callback recording is reproducible');
    }
  }
}

// Validate every record and hash the entire compressed file. Retain only case
// metadata and unresolved action IDs, never decoded witnesses or action history.
export async function readOriginalCapture(path) {
  const digest = createHash('sha256');
  const decoder = new StringDecoder('utf8');
  const input = createReadStream(path);
  input.on('data', bytes => digest.update(bytes));
  let sequence = 0, header, footer, active, nextCall = 0;
  const cases = [], open = new Map();
  function record(line) {
    const row = JSON.parse(line);
    assert.equal(row.sequence, sequence++);
    assert.equal(footer, undefined, 'no records after footer');
    if (!header) {
      assert.equal(row.type, 'header');
      assert.equal(row.format, format, 'explicit original capture format required');
      header = row.packet;
      assert.equal(row.caseCount, 5);
      return;
    }
    switch (row.type) {
      case 'case-start':
        assert.equal(active, undefined);
        assert.equal(row.case, cases.length);
        assert.equal(row.spec.completed, false);
        assert.equal(row.spec.sha256, createHash('sha256').update(row.spec.helpers + '\n' + row.spec.body).digest('hex'));
        active = {...row.spec};
        cases.push(active); nextCall = 0;
        break;
      case 'create':
        assert.ok(active);
        assert.equal(row.case, cases.length - 1);
        assert.equal(row.call, nextCall++);
        assert.equal(row.event.op, 'createGame');
        assert.ok(Object.hasOwn(row.event, 'initial'));
        break;
      case 'action-start':
        assert.ok(active);
        assert.equal(row.case, cases.length - 1);
        assert.equal(row.call, nextCall++);
        assert.ok(Object.hasOwn(row.event, 'before'));
        open.set(row.call, {op: row.event.op, game: row.event.game});
        break;
      case 'action-end':
        assert.ok(active);
        assert.equal(row.case, cases.length - 1);
        assert.deepEqual(open.get(row.call), {op: row.op, game: row.game});
        assert.ok(Object.hasOwn(row.event, 'after'));
        assert.ok(row.outcome === 'return' || row.outcome === 'throw');
        assert.equal(Object.hasOwn(row.event, 'failure'), row.outcome === 'throw');
        if (row.outcome === 'throw') assert.equal(Object.hasOwn(row.event, 'result'), false);
        // An undefined return was omitted by JSON.stringify in the old packet too.
        open.delete(row.call);
        break;
      case 'case-end':
        assert.ok(active);
        assert.equal(row.case, cases.length - 1);
        assert.equal(row.calls, nextCall);
        assert.equal(open.size, 0, 'every begun action has full after evidence');
        active.completed = row.completed;
        active = undefined;
        break;
      case 'footer':
        assert.equal(active, undefined);
        assert.equal(open.size, 0);
        assert.equal(row.cases, cases.length);
        assert.equal(cases.length, 5, 'all original callbacks required');
        assert.ok(cases.every(c => c.completed === true));
        assert.equal(row.failure, undefined, 'original callback failure retained, not admissible');
        assert.equal(row.sourceReadbackFailure, undefined);
        footer = row;
        break;
      default: assert.fail(`unknown original capture record: ${row.type}`);
    }
  }
  await pipeline(input, createGunzip(), async chunks => {
    let pending = '';
    for await (const bytes of chunks) {
      // JSON.stringify emits non-ASCII callback/source text. Decode incrementally
      // so a UTF-8 character split across decompressor chunks stays exact.
      pending += decoder.write(bytes);
      let end;
      while ((end = pending.indexOf('\n')) !== -1) {
        record(pending.slice(0, end));
        pending = pending.slice(end + 1);
      }
    }
    pending += decoder.end();
    assert.equal(pending, '', 'complete newline-terminated records required');
  });
  assert.ok(header && footer, 'complete original capture required');
  return {...header, cases, sha256Compressed: digest.digest('hex')};
}
