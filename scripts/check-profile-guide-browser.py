#!/usr/bin/env python3
"""Optional guided-profile acceptance with the offline ACP fixture, never real AI."""
import argparse
import importlib.util
import json
import re
from pathlib import Path
from playwright.sync_api import sync_playwright, expect


def run(args):
    args.output.mkdir(parents=True, exist_ok=True, mode=0o700)
    spec=importlib.util.spec_from_file_location('workspace',Path(__file__).with_name('check-workspace-browser.py'))
    helper=importlib.util.module_from_spec(spec);spec.loader.exec_module(helper)
    rows=[]
    with sync_playwright() as driver:
        browser=driver.chromium.launch(executable_path=str(args.chromium),chromium_sandbox=True)
        try:
            for width in [1440,390,320]:
                with helper.session(args.urdia,args.fake_agent,['--stream','--controls'],editor=True) as url:
                    page=browser.new_page(viewport={'width':width,'height':1000},reduced_motion='reduce');errors=[];posts=[]
                    page.on('pageerror',lambda _:errors.append(True))
                    page.on('request',lambda request: posts.append(request.url.split('/api/',1)[-1]) if request.method=='POST' and '/api/' in request.url else None)
                    page.goto(url);page.locator('.navigation [data-view="settings"]').click()
                    page.locator('#profile-editor > summary').click()
                    expect(page.locator('#profile-advanced')).not_to_have_attribute('open','')
                    expect(page.locator('#profile-edit-name')).to_be_visible()
                    assert posts==[], 'opening the form started work'
                    # Manual configuration remains usable without optional inspection.
                    page.locator('#profile-save').click()
                    expect(page.locator('#profile-advanced')).to_have_attribute('open','')
                    expect(page.locator('#profile-edit-model')).to_be_focused()
                    assert posts==[]
                    page.locator('#profile-advanced > summary').click()
                    page.locator('#control-probe-confirm').check();page.locator('#control-probe-start').click()
                    expect(page.locator('#control-model-options')).to_be_enabled(timeout=15000)
                    page.locator('#control-model-options').select_option('model-a(openai)')
                    generated=page.locator('#profile-edit-id').input_value()
                    assert re.fullmatch(r'[a-z0-9]+(?:-[a-z0-9]+)*',generated) and len(generated)<=128
                    assert page.locator('#profile-edit-name').input_value()
                    assert page.locator('#profile-edit-agent').input_value()
                    expect(page.locator('#profile-edit-inference')).to_have_value('Unknown')
                    expect(page.locator('#profile-edit-effort-selector')).to_have_value('')
                    assert posts==['profiles/inspect'], 'choosing a model dispatched work'
                    page.locator('#profile-edit-name').fill('Keep my chosen name')
                    page.locator('#control-probe-start').click()
                    expect(page.locator('#control-effort-options')).to_be_enabled(timeout=15000)
                    choices=page.locator('#control-effort-options option').evaluate_all('nodes=>nodes.map(n=>n.value).filter(Boolean)')
                    assert choices
                    page.locator('#control-effort-options').select_option(choices[0])
                    expect(page.locator('#profile-edit-name')).to_have_value('Keep my chosen name')
                    assert len(posts)==2
                    page.locator('#profile-advanced > summary').click()
                    page.locator('#profile-edit-model-selector').fill('model-b(openai)')
                    expect(page.locator('#profile-draft-summary')).to_contain_text('model-b(openai)')
                    expect(page.locator('#profile-edit-name')).to_have_value('Keep my chosen name')
                    page.locator('#profile-edit-model-selector').fill('model-a(openai)')
                    page.locator('#profile-edit-id').fill('')
                    # A deliberate empty ID must survive later suggestions.
                    page.locator('#profile-edit-effort-selector').fill(choices[-1])
                    expect(page.locator('#profile-edit-id')).to_have_value('')
                    page.locator('#profile-advanced > summary').click()
                    page.locator('#profile-save').click()
                    expect(page.locator('#profile-advanced')).to_have_attribute('open','')
                    expect(page.locator('#profile-edit-id')).to_be_focused()
                    assert len(posts)==2, 'invalid draft was sent'
                    page.locator('#profile-edit-id').fill('my-guided-profile')
                    page.locator('#profile-save').click()
                    expect(page.locator('#profile-save-status')).to_contain_text('Saved.',timeout=15000)
                    expect(page.locator('#catalog-status')).to_contain_text('Restart with the same profile store')
                    expect(page.locator('#profile-edit-select')).to_have_value('my-guided-profile')
                    expect(page.locator('#profile-edit-id')).to_have_attribute('readonly','')
                    page.locator('#profile-edit-name').fill('Unsaved existing-profile draft')
                    page.wait_for_timeout(1100)
                    expect(page.locator('#profile-edit-name')).to_have_value('Unsaved existing-profile draft')
                    assert posts==['profiles/inspect','profiles/inspect','profiles']
                    assert page.locator('.conversation-turn').count()==0
                    assert page.evaluate('document.documentElement.scrollWidth<=innerWidth'), width
                    assert page.evaluate('new Set([...document.querySelectorAll("[id]")].map(n=>n.id)).size===document.querySelectorAll("[id]").length')
                    assert not errors
                    page.locator('#profile-editor').screenshot(path=str(args.output/f'profile-guide-{width}.png'))
                    rows.append({'width':width,'explicit_fake_inspections_only':True,'suggestions_preserve_manual_and_empty':True,'advanced_validation_focus':True,'real_host_save_pending_restart':True,'poll_preserves_existing_edits':True,'no_task_or_permission':True})
                    page.close()
        finally:browser.close()
    (args.output/'evidence.json').write_text(json.dumps({'offline_fixture_not_AI':True,'checks':rows},indent=2)+'\n');print(json.dumps(rows))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['urdia','fake-agent','chromium','output']:parser.add_argument('--'+name,type=Path,required=True)
    run(parser.parse_args())
