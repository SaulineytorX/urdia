'use strict';
const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const {test} = require('node:test');
const source = fs.readFileSync(require('node:path').join(__dirname, '../../crates/urdia-ui/assets/live.js'), 'utf8');
const block = source.split('// BEGIN profile draft suggestions')[1].split('// END profile draft suggestions')[0];
const context = vm.createContext({TextEncoder});
vm.runInContext(block, context);
const suggest = input => JSON.parse(JSON.stringify(context.suggestProfileDraft(input)));
const base = {provider:{id:'qwen',name:'Qwen Code'},modelSelector:'model-a(openai)',effortSelector:'medium'};
test('missing inputs produce no invented metadata',()=>{
  for(const input of [undefined,null,{},false]) assert.deepEqual(suggest(input),{id:null,name:null,agent:null});
});
test('deterministic readable labels with exact input left intact',()=>{
  assert.deepEqual(suggest(base),{id:'model-a-openai',name:'Qwen Code · model-a(openai) · medium',agent:'Qwen Code'});
  assert.equal(base.modelSelector,'model-a(openai)');
});
test('manual ownership includes intentionally empty values',()=>{
  assert.deepEqual(suggest({...base,ownership:{id:'manual',name:'manual',agent:'manual'}}),{id:null,name:null,agent:null});
});
test('collisions remain host-valid at the truncation boundary',()=>{
  const id='a'.repeat(125)+'-xy';
  const result=suggest({...base,modelSelector:id,existingIds:[id]});
  assert.equal(result.id,'a'.repeat(125)+'-2');
  assert.match(result.id,/^[a-z0-9]+(?:-[a-z0-9]+)*$/);
  assert.ok(result.id.length<=128);
});
test('collision search is bounded and does not reuse IDs',()=>{
  assert.equal(suggest({...base,existingIds:['model-a-openai','model-a-openai-2']}).id,'model-a-openai-3');
  assert.equal(suggest({...base,existingIds:['model-a-openai',...Array.from({length:99},(_,i)=>'model-a-openai-'+(i+2))]}).id,null);
});
test('generated labels obey host UTF8 byte and control limits',()=>{
  const result=suggest({...base,provider:{id:'qwen',name:'😀'.repeat(100)+'\n\u0085'}});
  for (const value of [result.name,result.agent]) {
    assert.ok(Buffer.byteLength(value)<=128);assert.ok(!/[\x00-\x1f\x7f-\x9f]/u.test(value));
    assert.ok(!value.includes('\ufffd'));
  }
});
test('inert markup and non-ASCII slugs remain conservative',()=>{
  assert.equal(suggest({...base,modelSelector:'<>/ABC_123'}).id,'abc-123');
  assert.equal(suggest({provider:{id:'',name:'🦉'}}).id,null);
  assert.equal(suggest({...base,modelSelector:'123'}).id,'123');
});
