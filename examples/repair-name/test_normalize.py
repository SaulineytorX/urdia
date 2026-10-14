import unittest
from normalize import normalize_name
class Tests(unittest.TestCase):
 def test_spaces(self): self.assertEqual(normalize_name('  Ada   Lovelace  '),'ada lovelace')
 def test_unicode(self): self.assertEqual(normalize_name(' Straße '),'strasse')
 def test_empty(self):
  with self.assertRaises(ValueError):normalize_name('   ')
 def test_type(self):
  with self.assertRaises(TypeError):normalize_name(1)
