"""Regression fixtures for the real iFinD HTML and HTTP manual protocols."""

import html
import unittest

from scripts.generate_market_catalogue import extract_ifind


def highlighted(text):
    # The live manual splits punctuation and names across syntax-highlighting spans.
    return "<pre><code>" + "".join(
        f"<span>{html.escape(part)}</span>" for part in text.splitlines(keepends=True)
    ).replace("requestURL:", "requestURL</span><span>:").replace(
        "formData:", "formData</span><span>:"
    ) + "</code></pre>"


class IfindCatalogueTest(unittest.TestCase):
    def test_highlighted_manual_preserves_nested_indicator_arguments(self):
        manual = highlighted('''requestMethod:POST
requestURL:https://quantapi.51ifind.com/api/v1/basic_data_service
formData:{"codes":"300033.SZ","indipara":[{"indicator":"ths_close_price_stock","indiparams":["2025-01-13","100"]}]}''')
        entry = extract_ifind(manual, "")[0]
        self.assertEqual(entry["sample"], {
            "codes": "300033.SZ",
            "indipara": [{"indicator": "ths_close_price_stock", "indiparams": ["2025-01-13", "100"]}],
        })
        self.assertEqual(next(p for p in entry["params"] if p["name"] == "indipara")["type"], "array")
        self.assertTrue(next(p for p in entry["params"] if p["name"] == "codes")["required"])

    def test_python_example_keeps_nested_objects_and_braces_or_urls_in_strings(self):
        examples = highlighted('''thsUrl = 'https://quantapi.51ifind.com/api/v1/data_pool'
thsPara = {"reportname": "p03425", "functionpara": {"date": "20220706", "label": "a}b", "url": "https://example.invalid/a"}, "outputpara": "p03291_f002"}''')
        entry = extract_ifind("", examples)[0]
        self.assertIn("functionpara", entry["sample"])
        self.assertEqual(entry["sample"]["functionpara"], {
            "date": "20220706", "label": "a}b", "url": "https://example.invalid/a",
        })
        self.assertEqual(next(p for p in entry["params"] if p["name"] == "functionpara")["type"], "object")

    def test_http_manual_helpers_and_token_operations_have_safe_metadata(self):
        # Current official HTTP PDF v2.1, pages 2, 18, 30, 34. Endpoint names
        # come from that source, independently of the production fallback samples.
        manual = "\n".join("https://quantapi.51ifind.com/api/v1/" + name for name in [
            "get_access_token", "update_access_token", "portfolio_manage",
            "final_fund_valuation", "get_data_volume", "get_error_message", "get_thscode",
        ])
        entries = {e["path"].rsplit("/", 1)[-1]: e for e in extract_ifind(manual, "")}
        self.assertTrue({
            "get_access_token", "update_access_token", "portfolio_manage",
            "final_fund_valuation", "get_data_volume", "get_error_message", "get_thscode",
        }.issubset(entries))
        self.assertTrue(entries["portfolio_manage"]["mutating"])
        self.assertTrue(entries["update_access_token"]["mutating"])
        self.assertTrue(entries["get_access_token"].get("internal_auth"))
        self.assertTrue(entries["update_access_token"].get("internal_auth"))
        self.assertEqual(entries["get_access_token"]["auth"], "refresh_token")
        self.assertEqual(entries["update_access_token"]["auth"], "refresh_token")
        self.assertEqual(entries["get_access_token"]["params"], [])
        self.assertEqual(entries["update_access_token"]["params"], [])
        self.assertEqual(entries["get_error_message"]["sample"], {"errorcode": -1303})
        self.assertEqual(entries["get_data_volume"]["sample"], {})
        for entry in entries.values():
            self.assertEqual(entry["environments"], ["production"])

    def test_seven_previously_empty_data_forms_have_documented_fallbacks(self):
        names = ["basic_data_service", "date_sequence", "cmd_history_quotation",
                 "report_query", "fund_valuation", "data_pool", "get_trade_dates"]
        manual = "\n".join("https://quantapi.51ifind.com/api/v1/" + name for name in names)
        entries = {e["path"].rsplit("/", 1)[-1]: e for e in extract_ifind(manual, "")}
        for name in names:
            with self.subTest(name=name):
                self.assertTrue(entries[name]["params"])
                self.assertTrue(entries[name]["sample"])
        self.assertIn("indipara", entries["date_sequence"]["sample"])
        self.assertTrue({"codes", "startdate", "enddate", "indipara"}.issubset(
            {p["name"] for p in entries["date_sequence"]["params"] if p["required"]}
        ))
        self.assertIn("functionpara", entries["fund_valuation"]["sample"])
        self.assertTrue({"codes", "functionpara", "outputpara"}.issubset(
            {p["name"] for p in entries["fund_valuation"]["params"] if p["required"]}
        ))
        self.assertIn("outputpara", entries["data_pool"]["sample"])
        self.assertEqual(entries["report_query"]["title"], "专题报表")

    def test_timestamps_keep_the_documented_json_string_type(self):
        manual = highlighted('''requestURL:https://quantapi.51ifind.com/api/v1/high_frequency
formData:{"codes":"300033.SZ","indicators":"close","starttime":"2025-01-01 09:15:00","endtime":"2025-01-13 15:15:00"}''')
        entry = extract_ifind(manual, "")[0]
        parameters = {p["name"]: p for p in entry["params"]}
        self.assertIn("starttime", parameters)
        self.assertEqual(parameters["starttime"]["type"], "string")


if __name__ == "__main__":
    unittest.main()
