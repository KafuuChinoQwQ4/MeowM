#!/usr/bin/env python3
"""Offline contract checks for the OKX official-document catalogue generator."""

import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).with_name("generate_okx_catalogue.py")

# Sections deliberately contain response tables and cross-references; only the
# request metadata belongs in the callable catalogue.
FIXTURE = '''<!doctype html><html><body>
<h1 id="overview">概览</h1><h2>交易时效性</h2><h3 id="overview-transaction-timeouts-rest-api">REST API</h3>
<p>支持以下接口</p><ul><li><a href="#post-order">下单</a></li><li><a href="#batch-orders">批量下单</a></li></ul>
<h1 id="trading-account">交易账户</h1><h2 id="account-rest">REST API</h2>
<h3 id="account-balance">查看余额</h3>
<h4>限速：10次/2s</h4><h4>限速规则：User ID</h4>
<h4>HTTP请求</h4><p><code>GET /api/v5/account/balance</code></p>
<blockquote>请求示例</blockquote><div><pre class="shell">GET /api/v5/account/balance</pre></div>
<h4>请求参数</h4><table><thead><tr><th>参数名</th><th>类型</th><th>是否必须</th><th>描述</th></tr></thead><tbody>
<tr><td>ccy</td><td>String</td><td>否</td><td>币种，例如 <code>BTC</code></td></tr>
<tr><td>extra</td><td>Object</td><td>可选</td><td>额外参数</td></tr>
<tr><td>&gt; child</td><td>String</td><td>否</td><td>当 extra 存在时必填</td></tr>
<tr><td>&gt;&gt; leaf</td><td>String</td><td>是</td><td>子项</td></tr>
</tbody></table><blockquote>返回结果</blockquote><h4>返回参数</h4>
<table><tr><th>参数名</th><th>类型</th><th>描述</th></tr><tr><td>secretResponse</td><td>String</td><td>响应</td></tr></table>
<h3 id="cross-reference">文档引用</h3><p>其他接口 <code>GET /api/v5/fictional</code> 供参考。</p>
<h1 id="order-book-trading">撮合交易</h1><h2>交易</h2>
<h3 id="batch-orders">批量下单</h3><h4>HTTP请求</h4><p><code>POST /api/v5/trade/batch-orders</code></p>
<blockquote>请求示例</blockquote><div><pre class="shell">POST /api/v5/trade/batch-orders body
[{"instId":"BTC-USDT","sz":"1"}]</pre></div><h4>请求参数</h4>
<table><thead><tr><th>参数名</th><th>类型</th><th>是否必须</th><th>描述</th></tr></thead><tbody>
<tr><td>instId</td><td>String</td><td>是</td><td>产品 ID</td></tr>
<tr><td>sz</td><td>String</td><td>是</td><td>数量</td></tr></tbody></table>
<p>条件策略</p><table><thead><tr><th>参数名</th><th>类型</th><th>是否必须</th><th>描述</th></tr></thead><tbody>
<tr><td>triggerPx</td><td>String</td><td>是</td><td>触发价格</td></tr></tbody></table>
<h3 id="get-order">获取订单</h3><p><code>GET /api/v5/trade/order</code></p><h4>请求参数</h4><p>无</p>
<h3 id="post-order">下单</h3><p><code>POST /api/v5/trade/order</code></p><h4>请求参数</h4><p>无</p>
<h1 id="funding-account">资金账户</h1><h2>REST API</h2>
<h3 id="cancel-withdrawal">撤销提币</h3><p><code>POST /api/v5/asset/cancel-withdrawal</code></p><h4>请求参数</h4><p>无</p>
<h1 id="public-data">公共数据</h1><h2>REST API</h2>
<h3 id="public-post">公共查询</h3><h4>权限：公共</h4><p><code>POST /api/v5/public/query</code></p><h4>请求参数</h4><p>无</p>
<h3 id="public-estimate">计算最小投资数量（公共）</h3><p><code>POST /api/v5/tradingBot/grid/min-investment</code></p><h4>请求参数</h4><p>无</p>
<h3 id="public-leader">获取交易员排名</h3><p>公共接口，获取排名信息。</p><h4>限速规则：IP</h4><p><code>GET /api/v5/copytrading/public-lead-traders</code></p>
<h3 id="private-ip">获取 E-Mode</h3><h4>Permission: Read</h4><h4>限速规则：IP</h4><p><code>GET /api/v5/finance/flexible-loan/emode-info</code></p>
<h3 id="signed-market">签名查询</h3><h4>权限：读取</h4><p><code>GET /api/v5/market/requires-auth</code></p><h4>请求参数</h4><p>无</p>
<h2>WebSocket</h2>
<h3 id="candles">K线频道</h3><h4>URL Path</h4><p>/ws/v5/business</p>
<blockquote>请求示例</blockquote><div><pre class="shell">{"id":"1512","op":"subscribe","args":[{"channel":"candle1m","instId":"BTC-USDT"}]}</pre></div>
<h4>请求参数</h4><table><thead><tr><th>参数</th><th>类型</th><th>是否必须</th><th>描述</th></tr></thead><tbody>
<tr><td>id</td><td>String</td><td>否</td><td>标识</td></tr><tr><td>op</td><td>String</td><td>是</td><td>操作</td></tr>
<tr><td>args</td><td>Array of objects</td><td>是</td><td>订阅列表</td></tr>
<tr><td>&gt; channel</td><td>String</td><td>是</td><td>频道名<br><code>candle1m</code><br><code>candle5m</code><br><code>index -candle2H</code></td></tr>
<tr><td>&gt; instId</td><td>String</td><td>是</td><td>产品ID</td></tr></tbody></table>
<h3 id="order-book-trading-market-data-ws-order-book-channel">深度频道</h3>
<ul><li><code>books-rpi</code> 合并深度</li><li><code>books-elp</code> 已弃用</li></ul>
<aside>books-l2-tbt 和 books50-l2-tbt 频道需要 VIP4 登录。</aside><p>/ws/v5/public</p>
<blockquote>请求示例</blockquote><div><pre class="shell">{"op":"subscribe","args":[{"channel":"books","instId":"BTC-USDT"}]}</pre></div>
<h4>请求参数</h4><table><thead><tr><th>参数</th><th>类型</th><th>是否必须</th><th>描述</th></tr></thead><tbody>
<tr><td>&gt; channel</td><td>String</td><td>是</td><td>频道名 <code>books</code> <code>books-l2-tbt</code> <code>books50-l2-tbt</code></td></tr>
<tr><td>&gt; instId</td><td>String</td><td>是</td><td>产品ID</td></tr></tbody></table>
<h3 id="spread-books">价差深度</h3><p>/ws/v5/business</p>
<blockquote>请求示例</blockquote><div><pre class="shell">{"op":"subscribe","args":[{"channel":"sprd-books5","sprdId":"BTC-USDT_BTC-USDT-SWAP"}]}</pre></div>
<h4>请求参数</h4><table><thead><tr><th>参数</th><th>类型</th><th>是否必须</th><th>描述</th></tr></thead><tbody>
<tr><td>args</td><td>Array of objects</td><td>是</td><td>频道列表 <code>sprd-bbo-tbt</code> <code>sprd-books5</code> <code>sprd-books-l2-tbt</code></td></tr>
<tr><td>&gt; channel</td><td>String</td><td>是</td><td>频道名</td></tr></tbody></table>
<h3 id="sbe-feed">SBE 频道</h3><p>/ws/v5/public-sbe</p><blockquote>请求示例</blockquote>
<div><pre class="shell">{"op":"subscribe","args":[{"channel":"books-sbe"}]}</pre></div>
<h3 id="place-ws">WS 下单</h3><h4>服务地址</h4><p>/ws/v5/business (需要登录)</p>
<blockquote>请求示例</blockquote><div><pre class="shell">{"op":"sprd-order","args":[{"sprdId":"BTC-USDT_BTC-USDT-SWAP"}]}</pre></div>
<h4>请求参数</h4><table><thead><tr><th>Parameter</th><th>Type</th><th>Required</th><th>Description</th></tr></thead><tbody>
<tr><td>args</td><td>Array of objects</td><td>是</td><td>订单</td></tr><tr><td>&gt; sprdId</td><td>String</td><td>是</td><td>价差产品</td></tr>
<tr><td>&gt; ordId</td><td>String</td><td>按情况</td><td>订单标识</td></tr>
</tbody></table>
<h3 id="economic-calendar">经济日历频道</h3><p>/ws/v5/business (需要登录)</p>
<blockquote>请求示例</blockquote><div><pre class="shell">{"id":"1512"
"op":"subscribe","args":[{"channel":"economic-calendar"}]}</pre></div>
<h4>请求参数</h4><table><thead><tr><th>参数</th><th>类型</th><th>是否必须</th><th>描述</th></tr></thead><tbody>
<tr><td>&gt; channel</td><td>String</td><td>是</td><td>频道名 <code>economic-calendar</code></td></tr>
</tbody></table>
<h1>撮合交易</h1><h2 id="order-book-trading-sbe-market-data">SBE 行情数据</h2>
<h3 id="sbe-overview">概述</h3><ul><li><code>trades</code></li><li><code>bbo-tbt</code> 和 <code>books-l2-tbt</code></li></ul>
<h3 id="sbe-general">基本信息</h3><ul><li><code>bbo-tbt</code> 无用户等级限制，需登录；<code>trades</code> 和 <code>books-l2-tbt</code> 实盘 VIP4，模拟盘 VIP1。</li>
<li>实盘 <code>wss://ws.okx.com/ws/v5/public-sbe</code></li></ul>
<h3 id="sbe-integration">接入信息</h3><ul><li>WebSocket 握手请求头需要 API 密钥和签名。订阅请求使用 instIdCode。</li></ul>
<div><pre class="shell">订阅请求示例
{"op":"subscribe","args":[{"channel":"trades","instIdCode":211874}]}</pre></div>
</body></html>'''

# OKX also uses h2 for endpoint sections in a few top-level categories.  The
# fixture keeps an ordinary h2 group beside an h3 endpoint so the extractor
# must classify headings by their direct route, rather than by heading level.
H2_ENDPOINT_FIXTURE = '''<!doctype html><html><body>
<h1 id="support">支持</h1>
<h2 id="support-rest">REST API</h2>
<p>目录分组，不是接口。旧文档提到 <code>POST /api/v5/finance/stable-rewards/quote</code>、
<code>POST /api/v5/finance/stable-rewards/trade</code> 及
<code>GET /api/v5/finance/stable-rewards/subscribe-redeem-history</code>，但这些接口已停用。</p>
<h3 id="support-child">分组下的接口</h3>
<h4>HTTP请求</h4><p><code>GET /api/v5/support/child</code></p>
<h4>请求参数</h4><p>无</p>
<h2 id="announcement-get-announcements">GET / 公告</h2>
<p>该接口鉴权是可选的。</p>
<p>有 OK-ACCESS-KEY 时鉴权必须；没有时是公共接口，不需要鉴权。</p>
<h4>限速：5次/2s</h4><h4>HTTP请求</h4>
<p><code>GET /api/v5/support/announcements</code></p>
<h4>请求参数</h4><h4>请求示例</h4><table><tr><th>参数名</th><th>类型</th><th>是否必须</th><th>描述</th></tr>
<tr><td>annType</td><td>String</td><td>否</td><td>公告类型</td></tr></table>
<h2 id="announcement-get-announcement-types">GET / 公告类型</h2>
<p>公共接口不需要鉴权。</p><h4>HTTP请求</h4>
<p><code>GET /api/v5/support/announcement-types</code></p>
<h4>请求参数</h4><p>无</p>
<h1 id="status">Status</h1>
<h2 id="status-get-status">GET / Status</h2>
<h4>HTTP请求</h4><p><code>GET /api/v5/system/status</code></p>
<h4>请求参数</h4><p>无</p>
</body></html>'''


class CatalogueFixtureTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        directory = pathlib.Path(cls.tmp.name)
        source, cls.output = directory / "docs.html", directory / "catalogue.json"
        source.write_text(FIXTURE, encoding="utf-8")
        cls.process = subprocess.run(
            [sys.executable, str(SCRIPT), "--input", str(source), "--output", str(cls.output), "--retrieved-on", "2026-11-08"],
            capture_output=True, text=True, check=False,
        )

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def catalogue(self):
        self.assertEqual(self.process.returncode, 0, self.process.stderr)
        return json.loads(self.output.read_text(encoding="utf-8"))

    def test_rest_uses_only_standalone_routes_and_request_parameters(self):
        data = self.catalogue()
        rest = {endpoint["id"]: endpoint for endpoint in data["rest"]}
        self.assertEqual(set(rest), {"account-balance", "batch-orders", "get-order", "post-order", "cancel-withdrawal", "public-post", "public-estimate", "signed-market", "public-leader", "private-ip"})
        endpoint = rest["account-balance"]
        self.assertEqual((endpoint["category"], endpoint["group"], endpoint["title"]), ("交易账户", "REST API", "查看余额"))
        self.assertEqual((endpoint["method"], endpoint["path"], endpoint["auth"], endpoint["mutating"]), ("GET", "/api/v5/account/balance", "read", False))
        self.assertEqual(endpoint["rate_limit"], "限速：10次/2s；限速规则：User ID")
        self.assertTrue(endpoint["already_integrated"])
        self.assertEqual(endpoint["doc_url"], "https://www.okx.com/docs-v5/zh/#account-balance")
        self.assertEqual(endpoint["parameters"], [
            {"name":"ccy","type":"String","required":False,"conditional":False,"description":"币种，例如 BTC"},
            {"name":"extra","type":"Object","required":False,"conditional":False,"description":"额外参数"},
            {"name":"> child","type":"String","required":False,"conditional":True,"description":"当 extra 存在时必填"},
            {"name":">> leaf","type":"String","required":True,"conditional":False,"description":"子项"},
        ])

    def test_batch_payload_auth_and_method_specific_existing_coverage(self):
        rest = {endpoint["id"]: endpoint for endpoint in self.catalogue()["rest"]}
        self.assertEqual((rest["batch-orders"]["body_kind"], rest["batch-orders"]["auth"], rest["batch-orders"]["mutating"]), ("array", "trade", True))
        self.assertTrue(rest["get-order"]["already_integrated"])
        self.assertTrue(rest["post-order"]["already_integrated"])
        self.assertIn("supports_exp_time", rest["post-order"])
        self.assertTrue(rest["post-order"]["supports_exp_time"])
        self.assertTrue(rest["batch-orders"]["supports_exp_time"])
        self.assertFalse(rest["get-order"]["supports_exp_time"])
        self.assertFalse(rest["cancel-withdrawal"]["supports_exp_time"])
        self.assertEqual(rest["cancel-withdrawal"]["auth"], "withdraw")
        self.assertEqual((rest["public-post"]["auth"], rest["public-post"]["mutating"]), ("public", False))
        self.assertEqual((rest["public-estimate"]["auth"], rest["public-estimate"]["mutating"]), ("public", False))
        self.assertEqual(rest["signed-market"]["auth"], "read")
        self.assertEqual(rest["public-leader"]["auth"], "public")
        self.assertEqual(rest["private-ip"]["auth"], "read")
        self.assertIn({"name":"triggerPx","type":"String","required":True,"conditional":True,"description":"触发价格"}, rest["batch-orders"]["parameters"])

    def test_websocket_covers_concrete_channel_family_and_trading_operation(self):
        data = self.catalogue()
        self.assertEqual(data["source_url"], "https://www.okx.com/docs-v5/zh/")
        self.assertEqual(data["retrieved_on"], "2026-11-08")
        ws = {endpoint["id"]: endpoint for endpoint in data["websocket"]}
        self.assertEqual(set(ws), {"candles", "place-ws", "economic-calendar", "spread-books", "order-book-trading-market-data-ws-order-book-channel", "order-book-trading-market-data-ws-order-book-channel-vip", "order-book-trading-sbe-market-data-websocket"})
        candles = ws["candles"]
        self.assertEqual((candles["service"], candles["requires_login"], candles["operation"], candles["mutating"]), ("business", False, "subscribe", False))
        self.assertEqual(set(candles["channels"]), {"candle1m", "candle5m", "index-candle2H"})
        self.assertEqual(candles["sample_args"], [{"channel":"candle1m","instId":"BTC-USDT"}])
        self.assertIn({"name":"> instId","type":"String","required":True,"conditional":False,"description":"产品ID"}, candles["parameters"])
        self.assertEqual((ws["place-ws"]["service"], ws["place-ws"]["requires_login"], ws["place-ws"]["operation"], ws["place-ws"]["mutating"]), ("business", True, "sprd-order", True))
        self.assertEqual(ws["place-ws"]["sample_args"], [{"sprdId":"BTC-USDT_BTC-USDT-SWAP"}])
        self.assertIn({"name":"> ordId","type":"String","required":False,"conditional":True,"description":"订单标识"}, ws["place-ws"]["parameters"])
        self.assertEqual((ws["economic-calendar"]["operation"], ws["economic-calendar"]["requires_login"], ws["economic-calendar"]["channels"]), ("subscribe", True, ["economic-calendar"]))
        self.assertEqual(ws["economic-calendar"]["sample_args"], [{"channel":"economic-calendar"}])
        self.assertEqual(set(ws["spread-books"]["channels"]), {"sprd-bbo-tbt", "sprd-books5", "sprd-books-l2-tbt"})
        normal = ws["order-book-trading-market-data-ws-order-book-channel"]
        vip = ws["order-book-trading-market-data-ws-order-book-channel-vip"]
        self.assertEqual((normal["requires_login"], set(normal["channels"])), (False, {"books", "books-rpi", "books-elp"}))
        self.assertEqual((vip["requires_login"], set(vip["channels"])), (True, {"books-l2-tbt", "books50-l2-tbt"}))
        sbe = ws["order-book-trading-sbe-market-data-websocket"]
        self.assertEqual((sbe["service"], sbe["requires_login"], sbe["operation"]), ("public-sbe", True, "subscribe"))
        self.assertEqual(set(sbe["channels"]), {"trades", "bbo-tbt", "books-l2-tbt"})
        self.assertEqual(sbe["sample_args"], [{"channel":"trades", "instIdCode":211874}])
        self.assertIn({"name":"> instIdCode", "type":"Integer", "required":True, "conditional":False, "description":"产品 ID 唯一标识码。"}, sbe["parameters"])
        self.assertIn("VIP4", next(p["description"] for p in sbe["parameters"] if p["name"] == "> channel"))


class H2EndpointFixtureTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        directory = pathlib.Path(cls.tmp.name)
        source, cls.output = directory / "docs.html", directory / "catalogue.json"
        source.write_text(H2_ENDPOINT_FIXTURE, encoding="utf-8")
        cls.process = subprocess.run(
            [sys.executable, str(SCRIPT), "--input", str(source), "--output", str(cls.output), "--retrieved-on", "2026-11-08"],
            capture_output=True, text=True, check=False,
        )

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def catalogue(self):
        self.assertEqual(self.process.returncode, 0, self.process.stderr)
        return json.loads(self.output.read_text(encoding="utf-8"))

    def test_h2_endpoints_are_extracted_without_promoting_groups_or_retired_mentions(self):
        rest = {endpoint["id"]: endpoint for endpoint in self.catalogue()["rest"]}
        self.assertEqual(set(rest), {
            "support-child", "announcement-get-announcements",
            "announcement-get-announcement-types", "status-get-status",
        })
        self.assertEqual((rest["support-child"]["category"], rest["support-child"]["group"]), ("支持", "REST API"))
        self.assertEqual((rest["announcement-get-announcements"]["category"], rest["announcement-get-announcements"]["group"]), ("支持", ""))
        self.assertEqual((rest["announcement-get-announcements"]["path"], rest["announcement-get-announcements"]["auth"]), ("/api/v5/support/announcements", "public"))
        self.assertEqual([parameter["name"] for parameter in rest["announcement-get-announcements"]["parameters"]], ["annType"])
        self.assertEqual(rest["announcement-get-announcement-types"]["auth"], "public")
        self.assertEqual((rest["status-get-status"]["path"], rest["status-get-status"]["auth"]), ("/api/v5/system/status", "public"))
        self.assertFalse(any("stable-rewards" in endpoint["path"] for endpoint in rest.values()))


if __name__ == "__main__":
    unittest.main()
