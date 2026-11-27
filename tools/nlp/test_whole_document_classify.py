import unittest
from unittest.mock import patch

import whole_document_classify as whole


class WholeTests(unittest.TestCase):
    def test_prompt_contains_complete_posts_and_ids(self):
        sections = {'post-1': 'first text', 'post-2': 'last text'}
        prompt = whole.make_prompt('title', sections)
        for value in ('post-1', 'post-2', 'first text', 'last text'):
            self.assertIn(value, prompt)

    def test_exact_evidence(self):
        topic = whole.TAXONOMY['topics'][0]['id']
        sections = {'post-1': 'some exact text'}
        valid = {'topics': [{'topic_id': topic, 'evidence': [{'section_id': 'post-1', 'quote': 'exact text'}]}]}
        self.assertEqual(whole.validate(valid, sections, whole.TAXONOMY), valid)
        valid['topics'][0]['evidence'][0]['quote'] = 'not present'
        with self.assertRaises(ValueError):
            whole.validate(valid, sections, whole.TAXONOMY)

    def test_prompt_hash_changes(self):
        original = whole.prompt_hash()
        with patch.object(whole, 'PROMPT', whole.PROMPT + ' changed'):
            self.assertNotEqual(original, whole.prompt_hash())


if __name__ == '__main__':
    unittest.main()
