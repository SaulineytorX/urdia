#!/usr/bin/env python3
"""Optional offline UX acceptance; synthetic display states are not AI execution."""
import argparse
import importlib.util
import json
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

# Selectors follow the implemented outcome card. Existing consent forms stay real.
CARD = '#task-outcome'
CHANGE = '#outcome-change'
CHECK = '#outcome-check'


def run(args):
    args.output.mkdir(parents=True, exist_ok=True, mode=0o700)
    spec = importlib.util.spec_from_file_location('workspace_checks', Path(__file__).with_name('check-workspace-browser.py'))
    helper = importlib.util.module_from_spec(spec); spec.loader.exec_module(helper)
    observations = []
    with sync_playwright() as driver:
        browser = driver.chromium.launch(executable_path=str(args.chromium), chromium_sandbox=True)
        try:
            for width in [1440, 390, 320]:
                with helper.session(args.urdia, args.fake_agent, ['--stream'], verification='good') as url:
                    page = browser.new_page(viewport={'width':width, 'height':950}, reduced_motion='reduce')
                    errors = []; posts = []
                    page.on('pageerror', lambda _: errors.append(True))
                    page.on('request', lambda request: posts.append(request.url.split('/api/',1)[-1]) if request.method == 'POST' and '/api/' in request.url else None)
                    page.goto(url); expect(page.locator('#conversation-input')).to_be_visible()
                    expect(page.locator(CARD)).to_be_hidden()
                    expect(page.locator('#result-preview-panel')).to_be_hidden()
                    expect(page.locator('#verification-panel')).to_be_hidden()
                    expect(page.locator('#task-metrics')).to_be_hidden()
                    assert posts == [], 'opening the interface dispatched work'
                    page.locator('#conversation-input').fill('Offline outcome fixture: no model call.')
                    page.locator('#conversation-submit').click(); expect(page.locator('#conversation-state')).to_have_text('completed',timeout=15000)
                    expect(page.locator(CARD)).to_be_visible()
                    expect(page.locator(CARD)).to_contain_text('No checks run')
                    assert posts == ['conversation/prompt']
                    page.locator('#conversation-input').fill('Keep my existing next task')
                    page.locator(CHANGE).focus(); page.keyboard.press('Enter')
                    expect(page.locator('#conversation-input')).to_be_focused()
                    expect(page.locator('#conversation-input')).to_have_value('Keep my existing next task')
                    assert posts == ['conversation/prompt'], 'request-change sent a task'
                    page.locator(CHECK).click()
                    expect(page.locator('#verification-consent')).to_be_visible()
                    expect(page.locator('#verification-consent')).not_to_be_checked()
                    expect(page.locator('#verification-run')).to_be_disabled()
                    page.locator('#verification-consent').check(); page.locator('#verification-run').click()
                    expect(page.locator('#verification-status')).to_contain_text('Calculator checks passed',timeout=20000)
                    expect(page.locator('#outcome-check-state')).to_have_text('Checks passed')
                    assert posts == ['conversation/prompt','conversation/verify']
                    # Repeat polling of terminal checks must not execute them again.
                    page.wait_for_timeout(1100)
                    assert posts == ['conversation/prompt','conversation/verify']
                    assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), width
                    assert page.evaluate('new Set([...document.querySelectorAll("[id]")].map(n=>n.id)).size === document.querySelectorAll("[id]").length')
                    page.locator(CARD).scroll_into_view_if_needed()
                    page.screenshot(path=str(args.output/f'checked-{width}.png'),full_page=True)
                    page.locator('#outcome-check-details').evaluate('node=>node.open=false')
                    page.locator(CARD).screenshot(path=str(args.output/f'result-card-{width}.png'))
                    # Source/display adversaries: current host result is replaced ONLY for UI projection.
                    transform = [None]
                    def display_fixture(route):
                        response = route.fetch(); data = response.json()
                        if transform[0]: transform[0](data)
                        route.fulfill(response=response,json=data)
                    page.route('**/api/conversation',display_fixture)
                    def invalid_report(data):
                        data['verification']['latest']['report'] = None
                    transform[0] = invalid_report
                    expect(page.locator('#outcome-check-state')).not_to_have_text('Checks passed',timeout=10000)
                    def failed_display(data):
                        data['turns'][-1]['status'] = 'failed'
                    transform[0] = failed_display
                    expect(page.locator('#outcome-agent-state')).to_have_text('Failed',timeout=10000)
                    expect(page.locator('#outcome-check-state')).not_to_have_text('Checks passed')
                    def cancelled_display(data):
                        data['turns'][-1]['status'] = 'cancelled'
                    transform[0] = cancelled_display
                    expect(page.locator('#outcome-agent-state')).to_have_text('Cancelled',timeout=10000)
                    expect(page.locator('#outcome-check-state')).not_to_have_text('Checks passed')
                    transform[0] = None
                    expect(page.locator('#conversation-state')).to_have_text('completed',timeout=10000)
                    page.unroute('**/api/conversation',display_fixture)
                    # A new actual fixture task must never inherit the previous check pass.
                    page.locator('#conversation-input').fill('Next offline fixture, no AI.')
                    page.locator('#conversation-submit').click(); expect(page.locator('#conversation-state')).to_have_text('completed',timeout=15000)
                    expect(page.locator(CARD)).to_contain_text('Earlier',timeout=10000)
                    expect(page.locator('#outcome-check-state')).not_to_have_text('Checks passed')
                    expect(page.locator('#verification-consent')).not_to_be_checked()
                    assert not errors
                    observations.append({'width':width,'empty_state_no_work':True,'separate_completion_checks':True,'draft_and_keyboard_preserved':True,'real_offline_host_checks':True,'invalid_report_failed_cancelled_refuse_pass':True,'historical_pass_not_current':True,'unique_ids_no_overflow':True})
                    page.close()
        finally:
            browser.close()
    (args.output/'evidence.json').write_text(json.dumps({'fixture_not_AI':True,'checks':observations},indent=2)+'\n')
    print(json.dumps(observations))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['urdia','fake-agent','chromium','output']:
        parser.add_argument('--'+name,type=Path,required=True)
    run(parser.parse_args())
