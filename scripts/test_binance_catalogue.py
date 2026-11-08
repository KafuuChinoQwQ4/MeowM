import unittest

from scripts.generate_market_catalogue import extract_binance


class BinanceCatalogueTest(unittest.TestCase):
    def test_public_endpoints_and_environment_metadata_are_preserved(self):
        reference = '''
### Spot REST API
##### `GET /api/v3/ping`
Test Connectivity
### Spot WebSocket API (1.0.0)
##### `POST /orderList.place.oco`
Send new OCO order (TRADE)
### Futures (USDⓈ-M) WebSocket Market Streams (1.0.0)
##### `POST /{symbol}@markPrice@{updateSpeed}`
Mark price
### Alpha WebSocket Market Streams (1.0.0)
##### `POST /{symbol}@trade`
Trade
'''
        entries = extract_binance(reference, [])
        by_path = {(entry['transport'], entry['path']): entry for entry in entries}
        self.assertEqual(by_path[('rest', '/api/v3/ping')]['auth'], 'public')
        self.assertEqual(
            by_path[('rest', '/api/v3/ping')]['environments'],
            ['production', 'spot_testnet'],
        )
        self.assertTrue(by_path[('ws_api', '/orderList.place.oco')]['mutating'])
        self.assertEqual(by_path[('stream', '/{symbol}@markPrice@{updateSpeed}')]['host'], 'fapi')
        self.assertEqual(by_path[('stream', '/{symbol}@trade')]['host'], 'alpha')

    def test_portfolio_margin_uses_dedicated_host_and_rejects_unsupported_testnet(self):
        reference = '''
### Portfolio Margin REST API
##### `GET /papi/v1/account`
Account Information (USER_DATA)
'''
        entry = extract_binance(reference, [])[0]
        self.assertEqual(entry['host'], 'papi')
        self.assertEqual(entry['environments'], ['production'])


if __name__ == '__main__':
    unittest.main()
