import unittest

from classification_contract import input_hash, load_taxonomy, validate


class ClassificationContractTests(unittest.TestCase):
    def setUp(self):
        self.sections = {"post-1-part-1": "Rust supports async functions."}
        self.valid = {"topics": [{"topic_id": "async-concurrency", "evidence": [
            {"section_id": "post-1-part-1", "quote": "async functions"}]}]}

    def test_valid_and_unclassified(self):
        self.assertEqual(validate(self.valid, self.sections), self.valid)
        self.assertEqual(validate({"topics": []}, self.sections), {"topics": []})
        self.assertEqual(len(load_taxonomy()["topics"]), 14)

    def test_invalid_evidence_or_id(self):
        self.valid["topics"][0]["evidence"][0]["quote"] = "async futures"
        with self.assertRaises(ValueError):
            validate(self.valid, self.sections)
        self.valid["topics"][0]["topic_id"] = "invented"
        with self.assertRaises(ValueError):
            validate(self.valid, self.sections)

    def test_hash_includes_title_and_all_sections(self):
        original = input_hash("title", self.sections)
        self.assertNotEqual(original, input_hash("different", self.sections))
        self.assertNotEqual(original, input_hash("title", {**self.sections, "post-2": "later"}))


if __name__ == "__main__":
    unittest.main()
