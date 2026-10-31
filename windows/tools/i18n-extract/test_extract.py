"""Regression tests for the literal-array table shape added to NotchStrings."""
import unittest
from extract_swift_strings import Parser, tokenize


class ExtractTests(unittest.TestCase):
    def parse(self, source):
        parser = Parser(tokenize(source), "fixture.swift")
        parser.parse()
        return parser

    def test_array_constructor_keeps_unicode_formats_and_grouped_languages(self):
        parser = self.parse('''
        struct ExampleStrings { let label: String; let count: String }
        extension FeatureStrings {
          static func example(_ language: AppLanguage) -> ExampleStrings {
            let words: [String]
            switch language {
            case .enUS: words = ["Files", "%2$@ %1$d %%"]
            case .ja, .zhHK: words = ["日本語", "%1$d・%2$@"]
            }
            return ExampleStrings(label: words[0], count: words[1])
          }
        }''')
        accessor = parser.accessors[0]
        self.assertEqual(accessor.cases["enUS"][1].values["count"], "%2$@ %1$d %%")
        self.assertEqual(accessor.cases["ja"][1].values["label"], "日本語")
        self.assertEqual(accessor.cases["zhHK"][1].values["count"], "%1$d・%2$@")

    def test_nonliteral_array_is_not_guessed(self):
        parser = self.parse('''
        struct ExampleStrings { let label: String }
        extension FeatureStrings {
          static func example(_ language: AppLanguage) -> ExampleStrings {
            let words: [String]
            switch language { case .enUS: words = [computeLabel()] }
            return ExampleStrings(label: words[0])
          }
        }''')
        table = parser.accessors[0].cases["enUS"][1]
        self.assertIsNone(table.values["label"])
        self.assertIn("label", table.unresolved)

    def test_tokenizer_keeps_literal_filename_placeholders_and_escapes(self):
        values = [t.text for t in tokenize(r'"%year %s\n日本語" // comment') if t.kind == "string"]
        self.assertEqual(values, ["%year %s\n日本語"])


if __name__ == "__main__":
    unittest.main()
