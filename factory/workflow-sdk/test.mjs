import test from 'node:test';
import assert from 'node:assert/strict';
import {defineWorkflow, unit, barrier, nesting} from './index.mjs';
import {readFileSync, existsSync} from 'node:fs';
import {fileURLToPath} from 'node:url';

test('builders are data only and do not mint identities or executions', () => {
  for (const build of [defineWorkflow, unit, barrier, nesting]) {
    const input = {key:'source-owned'};
    assert.equal(build(input),input);
    assert.equal(Object.isFrozen(input),true);
    for (const bad of [null,undefined,1,'x',[]]) assert.throws(()=>build(bad),TypeError);
  }
});
test('all declared package resources and exports are delivered', () => {
  const root=new URL('./',import.meta.url);
  const pkg=JSON.parse(readFileSync(new URL('package.json',root),'utf8'));
  assert.equal(pkg.name,'@epilogos/factory-workflow');
  for (const item of Object.values(pkg.exports['.'])) assert.ok(existsSync(fileURLToPath(new URL(item,root))));
  for (const source of ['single.workflow.ts','plural.workflow.ts']) {
    assert.match(readFileSync(new URL(`examples/${source}`,root),'utf8'),/defineWorkflow/);
  }
});
