import unittest

import spark_choose

PAGE = {
    "url": "https://example.test", "title": "T", "text": "hello",
    "actions": [
        {"id": "e1", "kind": "click", "label": "Search"},
        {"id": "e2", "kind": "fill", "label": "Where to?"},
        {"id": "scroll_down", "kind": "scroll", "label": "Scroll down"},
        {"id": "wait", "kind": "wait", "label": "Wait"},
    ],
}


class ParseDecision(unittest.TestCase):
    def test_click(self):
        d = spark_choose.parse_decision({"operation": "CLICK", "target": "e1", "confidence": 0.8}, PAGE)
        self.assertEqual(d["choice"], "e1")

    def test_unoffered_target_rejected(self):
        with self.assertRaises(ValueError):
            spark_choose.parse_decision({"operation": "CLICK", "target": "e99", "confidence": 0.5}, PAGE)

    def test_wrong_kind_rejected(self):
        with self.assertRaises(ValueError):
            spark_choose.parse_decision({"operation": "TYPE_TEXT", "target": "e1", "confidence": 0.5}, PAGE)

    def test_scroll_control(self):
        d = spark_choose.parse_decision({"operation": "SCROLL_DOWN", "target": None, "confidence": 0.4}, PAGE)
        self.assertEqual(d["choice"], "scroll_down")

    def test_done(self):
        d = spark_choose.parse_decision({"operation": "DONE", "target": None, "confidence": 1}, PAGE)
        self.assertEqual(d["choice"], "DONE")

    def test_bad_confidence(self):
        with self.assertRaises(ValueError):
            spark_choose.parse_decision({"operation": "DONE", "target": None, "confidence": 2}, PAGE)


if __name__ == "__main__":
    unittest.main()
