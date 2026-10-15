"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");
const {test} = require("node:test");
const source = fs.readFileSync(require("node:path").join(__dirname,"../../crates/urdia-ui/assets/live.js"),"utf8");
const block = source.split("// BEGIN first-run observations:")[1].split("// END first-run observations.")[0];
const projection = block.slice(block.indexOf("\n") + 1);
const state = vm.runInNewContext(projection + "\nfirstRunObservations;");
const original = {provider_id:"qwen",provider_name:"Qwen Code",workspace_name:"Practice"};
const snapshot = () => ({context:original,turns:[]});
test("configuration alone never claims a successful connection or checked output",()=>{
 const result=state(snapshot(),true,false);
 assert.equal(result.rows.agent.state,"attention"); assert.equal(result.rows.check.state,"waiting");
 assert.equal(result.canPrepare,true);
});
test("missing or disconnected host is not a readiness certificate",()=>{
 for(const input of [{},snapshot()])assert.equal(state(input,false,false).canPrepare,false);
 assert.equal(state({},true,false).rows.project.state,"waiting");
});
test("demo disclosure blocks coding preparation without blocking real offline model IDs",()=>{
 for(const context of [{...original,provider_id:"preview-demo"},{...original,provider_name:"Offline reference agent (not AI)"}])assert.equal(state({context,turns:[]},true,false).canPrepare,false);
 assert.equal(state({context:{...original,provider_id:"offline",provider_name:"Local coding model"},turns:[]},true,false).canPrepare,true);
});
test("pending setup and mismatched model pins refuse sample preparation",()=>{
 assert.equal(state(snapshot(),true,true).canPrepare,false);
 assert.equal(state({...snapshot(),live_controls:{explicit_configuration_matches:false}},true,false).canPrepare,false);
});
test("a response from another selected profile does not confirm this agent connection",()=>{
 const s=snapshot(); s.turns=[{turn_id:"one",original_session:true,status:"completed"}];
 s.profile_selection={selected_profile:"other",choices:{other:{...original,provider_id:"codex",profile:{id:"other",name:"Codex low"}}}};
 assert.equal(state(s,true,false).rows.agent.state,"attention");
});
test("agent completion is separated from verification",()=>{
 const s=snapshot();s.turns=[{turn_id:"one",original_session:true,status:"completed"}];
 const result=state(s,true,false);assert.equal(result.rows.agent.state,"ready");assert.equal(result.rows.check.state,"attention");
});
test("failed tasks expose a review state, never a connection pass",()=>{
 const s=snapshot();s.turns=[{turn_id:"one",original_session:true,status:"failed"}];
 assert.equal(state(s,true,false).rows.agent.state,"failed");
});
test("only current host checks with unchanged files get a pass",()=>{
 const s=snapshot();s.turns=[{turn_id:"one",original_session:true,status:"completed"}];
 s.verification={latest:{turn_id:"one",original_session:true,status:"passed",stale:false,report:{status:"passed",acceptance_files_unchanged:true}}};
 assert.equal(state(s,true,false).rows.check.state,"passed");
 for(const mutate of [s=>s.verification.latest.stale=true,s=>s.turns.push({turn_id:"two",original_session:true,status:"completed"}),s=>s.verification.latest.report.acceptance_files_unchanged=false,s=>s.verification.latest.report=null,s=>s.turns[0].status="failed"]){
  const changed=JSON.parse(JSON.stringify(s));mutate(changed);assert.notEqual(state(changed,true,false).rows.check.state,"passed");
 }
});

test("restored turns and changed model pins do not certify a current connection",()=>{
 const s=snapshot();s.turns=[{turn_id:"old",original_session:false,status:"completed"}];
 assert.equal(state(s,true,false).rows.agent.state,"attention");
 s.turns[0].original_session=true;s.live_controls={explicit_configuration_matches:false};
 assert.equal(state(s,true,false).rows.agent.state,"attention");
});

test("manual mode shows reported model without assuming defaults or verified inference",()=>{
 const s=snapshot();s.context={...original,startup_controls:{model:"reported-model"}};
 assert.match(state(s,true,false).rows.profile.text,/Agent reports: reported-model/);
 assert.equal(state(s,true,false).rows.profile.state,"waiting");
});
