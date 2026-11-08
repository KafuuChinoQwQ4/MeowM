import unittest

from scripts.generate_market_catalogue import extract_binance, extract_ifind


class OfficialCatalogueTest(unittest.TestCase):
    def test_binance_distinguishes_rest_signed_public_and_ws_protocols(self):
        reference = '''\n## API Reference\n\n### Spot REST API (1.0.0)\n\n#### market\n\n##### `GET /api/v3/ticker/price`\n\n价格\n\nOperation ID: `price`\n\n##### `POST /api/v3/order`\n\n下单 (TRADE)\n\nOperation ID: `order`\n\n### Spot WebSocket Market Streams (1.0.0)\n\n#### market\n\n##### `POST /{symbol}@trade`\n\n逐笔交易\n\nOperation ID: `trade`\n'''
        collection = {'item': [{'name': 'Trades', 'item': [
            {'name': 'Price', 'request': {'method': 'GET', 'url': {'raw': '{{url}}/api/v3/ticker/price', 'query': [{'key': 'symbol', 'value': 'BTCUSDT'}]}}},
            {'name': 'Order', 'request': {'method': 'POST', 'url': {'raw': '{{url}}/api/v3/order', 'query': [{'key': 'symbol', 'value': 'BTCUSDT'}, {'key': 'signature', 'value': ''}, {'key': 'timestamp', 'value': ''}]}}},
        ]}]}
        entries = extract_binance(reference, [('spot.json', collection)])
        self.assertEqual([(x['transport'], x['auth']) for x in entries], [('rest', 'public'), ('rest', 'trade'), ('stream', 'public')])
        self.assertEqual(entries[1]['params'][0]['name'], 'symbol')
        self.assertTrue(entries[1]['mutating'])

        extended = extract_binance(
            '```\n`GET /bapi/defi/v1/public/alpha-trade/fullDepth`\n```',
            [],
        )
        self.assertEqual(extended[0]['host'], 'alpha')
        self.assertEqual(extended[0]['auth'], 'public')

    def test_ifind_extracts_documented_json_examples_only(self):
        manual = '''<h1>数据函数</h1><h2 id="quote">实时行情：THS_RQ</h2><pre>requestMethod:POST\nrequestURL:https://quantapi.51ifind.com/api/v1/real_time_quotation\nformData:{"codes":"300033.SZ","indicators":"latest"}</pre><h2>SDK only</h2>'''
        examples = '''<pre>def pool():\n    thsUrl = 'https://quantapi.51ifind.com/api/v1/data_pool'\n    thsPara = {"reportname": "p03425"}\n    requests.post(url=thsUrl, json=thsPara)</pre>'''
        entries = extract_ifind(manual, examples)
        self.assertEqual([e['path'] for e in entries], ['/api/v1/real_time_quotation', '/api/v1/data_pool'])
        self.assertEqual(entries[0]['sample'], {'codes': '300033.SZ', 'indicators': 'latest'})
        self.assertEqual(entries[1]['sample'], {'reportname': 'p03425'})


if __name__ == '__main__':
    unittest.main()
