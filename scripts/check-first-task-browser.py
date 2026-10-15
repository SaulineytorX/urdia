#!/usr/bin/env python3
"""Optional credential-free first-task UI acceptance using the offline ACP fixture.

Synthetic provider-label states are explicitly fixtures, not real AI execution.
Requires the same private Playwright/Chromium environment as workspace acceptance.
"""
import argparse
import importlib.util
import json
from pathlib import Path
from playwright.sync_api import sync_playwright, expect


def run(args):
    args.output.mkdir(parents=True, exist_ok=True, mode=0o700)
    spec = importlib.util.spec_from_file_location("workspace_checks", Path(__file__).with_name("check-workspace-browser.py"))
    helper = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(helper)
    checks = []
    with sync_playwright() as driver:
        browser = driver.chromium.launch(executable_path=str(args.chromium), chromium_sandbox=True)
        helper.check_setup_guide(browser, args)
        for width in [1440, 390, 320]:
            with helper.session(args.urdia, args.fake_agent, ["--stream"], verification="good") as url:
                page = browser.new_page(viewport={"width":width,"height":900}, reduced_motion="reduce")
                errors = []
                page.on("pageerror", lambda error: errors.append(str(error)))
                page.goto(url)
                page.locator('[data-view="setup"]').first.click()
                expect(page.locator('#first-run-agent-status')).to_contain_text('Offline demo')
                page.locator('#first-task-confirm').check()
                expect(page.locator('#first-task-prepare')).to_be_disabled()
                # Identity replacement tests frontend presentation only; execution stays the offline fixture.
                def real_identity(route):
                    response = route.fetch()
                    data = response.json()
                    data['context']['provider_id'] = 'qwen-fixture'
                    data['context']['provider_name'] = 'Qwen fixture (not actual AI)'
                    route.fulfill(response=response, json=data)
                page.route('**/api/conversation', real_identity)
                expect(page.locator('#first-task-prepare')).to_be_enabled(timeout=10000)
                expect(page.locator('#first-run-agent-status')).to_contain_text('response not confirmed')
                page.locator('#first-task-prepare').focus()
                page.keyboard.press('Enter')
                expect(page.locator('#conversation-input')).to_have_value(__import__('re').compile('Create calculator.py with add'))
                expect(page.locator('#verification-contract')).to_have_value('calculator-add-v1')
                expect(page.locator('#verification-consent')).not_to_be_checked()
                assert page.locator('.conversation-turn').count() == 0, 'preparing a draft sent a task'
                page.locator('[data-view="setup"]').first.click()
                expect(page.locator('#first-task-prepare')).to_be_disabled()
                expect(page.locator('#first-task-note')).to_contain_text('Sample prepared only')
                page.locator('[data-view="workspace"]').first.click()
                page.locator('#conversation-input').fill('Keep my existing draft')
                page.locator('[data-view="setup"]').first.click()
                page.locator('#first-task-confirm').check()
                expect(page.locator('#first-task-prepare')).to_be_disabled()
                page.locator('[data-view="workspace"]').first.click()
                expect(page.locator('#conversation-input')).to_have_value('Keep my existing draft')
                page.locator('#conversation-submit').click()
                expect(page.locator('.conversation-turn')).to_contain_text('completed', timeout=15000)
                page.locator('[data-view="setup"]').first.click()
                expect(page.locator('#first-run-check-status')).to_contain_text('independent checks have not run')
                page.locator('[data-view="workspace"]').first.click()
                page.locator('#verification-review').evaluate('n=>n.open=true')
                page.locator('#verification-consent').check()
                page.locator('#verification-run').click()
                expect(page.locator('#verification-status')).to_contain_text('Calculator checks passed', timeout=20000)
                page.locator('[data-view="setup"]').first.click()
                expect(page.locator('#first-run-check-status')).to_contain_text('Checks passed for the latest task')
                assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), width
                assert page.evaluate('new Set([...document.querySelectorAll("[id]")].map(n=>n.id)).size === document.querySelectorAll("[id]").length')
                assert not errors, errors
                page.screenshot(path=str(args.output / f'first-task-{width}.png'), full_page=True)
                # A subsequent task must invalidate the visible historical pass.
                page.locator('[data-view="workspace"]').first.click()
                page.locator('#conversation-input').fill('Another explicit fixture task')
                page.locator('#conversation-submit').click()
                expect(page.locator('.conversation-turn').last).to_contain_text('completed', timeout=15000)
                page.locator('[data-view="setup"]').first.click()
                expect(page.locator('#first-run-check-status')).to_contain_text('Earlier checks')
                page.close()
                checks.append({'viewport':width,'inert_preparation':True,'demo_block':True,'draft_preservation':True,'separate_real_host_checks':True,'historical_pass_invalidated':True})
        browser.close()
    (args.output/'evidence.json').write_text(json.dumps({'synthetic_agent_identity':True,'checks':checks},indent=2)+'\n')
    print(json.dumps(checks))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['urdia','fake-agent','chromium','output']:
        parser.add_argument('--'+name, type=Path, required=True)
    run(parser.parse_args())
