"""Native acceptance must not invent a Git identity for delivered source archives."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'check-native-secret-service.py'
spec = importlib.util.spec_from_file_location('native_vault', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class SourceIdentityTests(unittest.TestCase):
    def test_archive_inside_another_repository_does_not_inherit_its_identity(self):
        with tempfile.TemporaryDirectory() as raw:
            parent = Path(raw)
            subprocess.run(['git', 'init', '-q', str(parent)], check=True)
            extracted = parent / 'extracted'
            extracted.mkdir()
            with patch.object(module, 'ROOT', extracted):
                self.assertEqual(module.source_identity(), (None, None))

    def test_real_checkout_records_clean_and_dirty_states(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            subprocess.run(['git', 'init', '-q', str(root)], check=True)
            subprocess.run(['git', '-c', 'user.name=Offline', '-c', 'user.email=offline@example.invalid', '-c', 'commit.gpgsign=false', 'commit', '--allow-empty', '-qm', 'fixture'], cwd=root, check=True)
            expected = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
            with patch.object(module, 'ROOT', root):
                self.assertEqual(module.source_identity(), (expected, False))
                (root / 'untracked').write_text('fixture')
                self.assertEqual(module.source_identity(), (expected, True))

    def test_broken_git_metadata_stays_unknown(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            (root / '.git').write_text('not a repository')
            with patch.object(module, 'ROOT', root):
                self.assertEqual(module.source_identity(), (None, None))


if __name__ == '__main__':
    unittest.main()
