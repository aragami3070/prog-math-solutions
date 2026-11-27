import unittest
from unittest.mock import patch

import batch_classify as batch


class BatchTests(unittest.TestCase):
    def test_chunks_preserve_every_character(self):
        sections = {'post-1': 'a' * (batch.CHUNK_SIZE + 3), 'post-2': 'xyz'}
        parts = batch.chunks(sections)
        self.assertEqual(list(parts[1]), ['post-1-part-2'])
        self.assertEqual(''.join(next(iter(p.values())) for p in parts), ''.join(sections.values()))

    def test_union_refuses_silent_truncation(self):
        results = [{'topics': [{'topic_id': str(i), 'evidence': [{'section_id': 's', 'quote': 'q'}]} for i in range(3)]},
                   {'topics': [{'topic_id': str(i), 'evidence': [{'section_id': 's', 'quote': 'q'}]} for i in range(3, 6)]}]
        with self.assertRaises(ValueError):
            batch.aggregate(results)

    def test_successful_response_with_quota_word_is_not_rate_limit(self):
        class Process:
            returncode = 0
            stderr = ''
            stdout = '{"status":"SUCCESS","response":"quota","structured_output":{"topics":[]}}'
        with patch('batch_classify.subprocess.run', return_value=Process()):
            result, _, error = batch.call_agy('title', {'s': 'body'}, 1)
        self.assertEqual(result, {'topics': []})
        self.assertIsNone(error)

    def test_invalid_output_retains_usage(self):
        class Process:
            returncode = 0
            stderr = ''
            stdout = '{"status":"SUCCESS","usage":{"tokens":17},"structured_output":{"topics":[{}]}}'
        with patch('batch_classify.subprocess.run', return_value=Process()):
            result, usage, error = batch.call_agy('title', {'s': 'body'}, 1)
        self.assertIsNone(result)
        self.assertEqual(usage, {'tokens': 17})
        self.assertIn('invalid output', error)


if __name__ == '__main__':
    unittest.main()
