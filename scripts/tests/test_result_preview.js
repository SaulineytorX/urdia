const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync('crates/urdia-ui/assets/live.js', 'utf8');
const begin = source.indexOf('  // BEGIN result preview URL validation');
const end = source.indexOf('  // END result preview URL validation');
assert.ok(begin >= 0 && end > begin);
const context = {URL};
vm.createContext(context);
vm.runInContext(source.slice(begin, end), context);
const host = `p-${'a'.repeat(48)}.localhost`;
test('only the exact private preview URL shape is opened', () => {
  assert.equal(context.resultPreviewUrl(`http://${host}:49152/`), `http://${host}:49152/`);
  for (const value of [null, '', 'javascript:alert(1)', `https://${host}:49152/`,
    'http://127.0.0.1:1234/', 'http://localhost:1234/',
    `http://${host}.attacker.test:1234/`, `http://${host}/`,
    `http://${host}:1234/path`, `http://${host}:1234/?token=x`,
    `http://${host}:1234/#token=x`, `http://user:password@${host}:1234/`,
    `http://p-${'a'.repeat(47)}.localhost:1234/`]) {
    assert.equal(context.resultPreviewUrl(value), null, String(value));
  }
});
