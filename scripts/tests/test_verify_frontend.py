"""Frontend source snapshot safety does not require third-party browser tooling."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('frontend',ROOT/'scripts/verify-frontend.py');checker=importlib.util.module_from_spec(spec);spec.loader.exec_module(checker)
class SnapshotTests(unittest.TestCase):
 def test_excludes_hidden_credentials_and_vendor_code(self):
  with tempfile.TemporaryDirectory() as raw:
   root=Path(raw);(root/'index.html').write_text('<p>hello</p>');(root/'.env').write_text('private');(root/'node_modules').mkdir();(root/'node_modules/vendor.js').write_text('private')
   self.assertEqual(set(checker.snapshot(root)),{'index.html'})
 def test_redirected_and_oversized_assets_are_refused(self):
  with tempfile.TemporaryDirectory() as raw:
   root=Path(raw);(root/'index.html').write_text('<p>hello</p>');target=root/'target.txt';target.write_text('not served');(root/'app.js').symlink_to(target)
   with self.assertRaises(OSError):checker.snapshot(root)
   (root/'app.js').unlink();(root/'app.js').write_bytes(b'x'*1048577)
   with self.assertRaises(ValueError):checker.snapshot(root)
 def test_no_consent_does_not_read_workspace_or_import_browser(self):
  result=subprocess.run([sys.executable,'-I','-B',str(ROOT/'scripts/verify-frontend.py'),'--workspace','/nonexistent/private','--contract','frontend-static-v1','--browser','/nonexistent'],capture_output=True,timeout=5)
  self.assertEqual(result.returncode,2);self.assertEqual(result.stdout,b'')
if __name__=='__main__':unittest.main()
