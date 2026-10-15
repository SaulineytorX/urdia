'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync('crates/urdia-ui/assets/live.js','utf8');
const section = (start,end) => source.split(start)[1].split(end)[0];
const pure = section('// BEGIN result preview URL validation','// END result preview URL validation') + '\n' + section('// BEGIN task outcome projection','// END task outcome projection');
const state = vm.runInNewContext(pure+'\ntaskOutcome;', {URL});
const snapshot = () => ({turns:[{turn_id:'current',status:'completed'}],verification:{active:false,latest:null},result_preview:{preparing:false,latest:null}});
const passed = () => {
 const s=snapshot();s.verification.latest={turn_id:'current',contract:'python-unittest-v1',status:'passed',stale:false,report:{contract:'python-unittest-v1',status:'passed',acceptance_files_unchanged:true}};return s;
};
test('empty, malformed and active snapshots never claim a finished result',()=>{
 for(const s of [null,{}, {turns:[]}, {turns:[null]}, {turns:[{status:'completed'}]}, {turns:[{status:'running'}]}, {...passed(),active_turn_id:'other'}]) assert.equal(state(s).visible,false);
});
test('agent completion is separate from scoped passing checks',()=>{
 assert.equal(state(snapshot()).currentCheckPassed,false);
 assert.equal(state(snapshot()).checkLabel,'No checks run');
 assert.equal(state(passed()).currentCheckPassed,true);
});
test('missing, changed, mismatched and historical reports never inherit a pass',()=>{
 for(const mutate of [s=>s.verification.latest.stale=true,s=>s.verification.latest.turn_id='old',s=>s.verification.latest.report=null,s=>s.verification.latest.report.status='failed',s=>s.verification.latest.report.acceptance_files_unchanged=false,s=>s.verification.latest.report.contract='other',s=>s.verification.latest.status='running',s=>s.turns[0].status='failed',s=>s.turns[0].status='cancelled']) {
  const s=passed();mutate(s);assert.equal(state(s).currentCheckPassed,false);assert.notEqual(state(s).checkTone,'success');
 }
});
test('failed and cancelled tasks have distinct non-success outcomes',()=>{
 for(const status of ['failed','cancelled']) {
  const s=passed();s.turns[0].status=status;const result=state(s);
  assert.equal(result.visible,true);assert.equal(result.outcomeState,status);assert.equal(result.previewVisible,false);assert.notEqual(result.agentTone,'success');
 }
});
test('unavailable host capabilities do not offer invented actions',()=>{
 const result=state({turns:[{turn_id:'one',status:'completed'}]});assert.equal(result.checkVisible,false);assert.equal(result.previewVisible,false);
});
test('preview readiness requires an exact current turn and validated private link',()=>{
 const s=snapshot();s.result_preview.latest={turn_id:'current',current_task:true,status:'ready',url:`http://p-${'a'.repeat(48)}.localhost:54321/`,expires_in_seconds:90};
 assert.equal(state(s).currentPreviewReady,true);
 for(const mutate of [s=>s.result_preview.latest.current_task=false,s=>s.result_preview.latest.turn_id='old',s=>delete s.result_preview.latest.turn_id,s=>s.result_preview.latest.url='http://127.0.0.1:1234/',s=>s.result_preview.latest.status='expired',s=>s.result_preview.latest.status='stopped']) {
  const c=structuredClone(s);mutate(c);assert.equal(state(c).currentPreviewReady,false);
 }
});
test('preview expiry updates without needing a new sequence',()=>{
 const s=snapshot();s.sequence=7;s.result_preview.latest={turn_id:'current',current_task:true,status:'expired'};
 assert.match(state(s).previewMeta,/expired/);s.result_preview.latest.status='stopped';assert.match(state(s).previewMeta,/stopped/);
});
test('viewing checks never becomes authorization and changes wait for host operations',()=>{
 for(const key of ['checkDisabled','previewDisabled']) assert.equal(state(snapshot())[key],false);
 for(const mutate of [s=>s.verification.active=true,s=>s.result_preview.preparing=true,s=>s.reconnection={status:'reconnecting'}]) {
  const s=snapshot();mutate(s);assert.equal(state(s).changeDisabled,true);
 }
});

test('an older live preview remains stoppable after failure or cancellation',()=>{
 for(const status of ['failed','cancelled']) {
  const s=snapshot();s.turns[0].status=status;s.result_preview.latest={turn_id:'old',current_task:false,status:'ready'};
  const result=state(s);assert.equal(result.previewVisible,true);assert.equal(result.currentPreviewReady,false);assert.match(result.previewActionLabel,/earlier/);
 }
});
