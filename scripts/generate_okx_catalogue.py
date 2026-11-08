#!/usr/bin/env python3
"""Generate the checked-in OKX v5 catalogue from a downloaded official HTML page.

Requires Python 3 and Beautiful Soup 4 (install with ``pip install beautifulsoup4``).
No network is used: download https://www.okx.com/docs-v5/zh/ separately, then run
``python3 scripts/generate_okx_catalogue.py --input docs.html --retrieved-on YYYY-MM-DD``.
The output defaults to the Rust connector's catalogue.json.  A local HTML snapshot
and explicit retrieval date make regeneration reviewable and reproducible.
"""

import argparse
import datetime as dt
import json
import re
import sys
from pathlib import Path

try:
    from bs4 import BeautifulSoup
except ImportError as exc:
    raise SystemExit("Beautiful Soup 4 is required: pip install beautifulsoup4") from exc

SOURCE_URL = "https://www.okx.com/docs-v5/zh/"
DEFAULT_OUTPUT = Path(__file__).resolve().parents[1] / "apps/tauri-ui/src-tauri/src/connectors/markets/okx/catalogue.json"
ROUTE = re.compile(r"(GET|POST)\s+(/api/v5/[^\s?]+)(?:\?[^\s]*)?", re.IGNORECASE)
WS_SERVICE = re.compile(r"/ws/v5/(public|private|business)(?![a-zA-Z0-9_-])", re.IGNORECASE)

# Existing connector calls. A path can be both a read and a write operation.
INTEGRATED_GET = {
    "/api/v5/account/balance", "/api/v5/account/config", "/api/v5/account/positions",
    "/api/v5/account/positions-history", "/api/v5/account/leverage-info", "/api/v5/account/bills",
    "/api/v5/account/bills-archive", "/api/v5/asset/balances", "/api/v5/asset/asset-valuation",
    "/api/v5/asset/deposit-history", "/api/v5/asset/withdrawal-history", "/api/v5/market/tickers",
    "/api/v5/market/candles", "/api/v5/market/history-candles", "/api/v5/public/instruments",
    "/api/v5/trade/order", "/api/v5/trade/orders-pending", "/api/v5/trade/orders-history",
    "/api/v5/trade/fills-history",
}
INTEGRATED_POST = {"/api/v5/trade/order", "/api/v5/trade/cancel-order"}
PUBLIC_PATHS = {
    "/api/v5/support/announcements",
    "/api/v5/support/announcement-types",
    "/api/v5/system/status",
}


def text(node):
    return re.sub(r"\s+", " ", node.get_text(" ", strip=True)).strip()


def section_nodes(heading):
    for node in heading.next_siblings:
        name = getattr(node, "name", None)
        if name in {"h1", "h2", "h3"}:
            break
        if name:
            yield node


def request_nodes(nodes):
    """Yield the documented request-parameter section, stopping at the response."""
    inside = False
    for node in nodes:
        value = text(node) if node.name in {"h4", "h5", "blockquote"} else ""
        if node.name in {"h4", "h5"} and "请求参数" in value:
            inside = True
            continue
        if inside and node.name in {"h4", "h5"}:
            # A few official sections put a second "请求示例" heading
            # immediately after "请求参数" and before the parameter table.
            # It is still part of the request section; response headings are
            # the boundary that ends extraction.
            if "请求示例" in value:
                continue
            break
        if inside and node.name == "blockquote" and re.search(r"返回|响应|推送", value):
            break
        if inside:
            yield node


def extract_parameters(nodes):
    parameters, channel_values = [], []
    for table_number, table in enumerate(node for node in request_nodes(nodes) if node.name == "table"):
        rows = table.find_all("tr")
        if not rows:
            continue
        headers = [text(cell) for cell in rows[0].find_all(["th", "td"], recursive=False)]
        name_index = next((i for i, h in enumerate(headers) if h in {"参数", "参数名", "参数名称"} or h.lower() == "parameter"), None)
        type_index = next((i for i, h in enumerate(headers) if "类型" in h or h.lower() == "type"), None)
        required_index = next((i for i, h in enumerate(headers) if "必须" in h or "必填" in h or h.lower() == "required"), None)
        description_index = next((i for i, h in enumerate(headers) if "描述" in h or "说明" in h or h.lower() == "description"), None)
        if name_index is None or type_index is None or description_index is None:
            continue
        for row in rows[1:]:
            cells = row.find_all(["td", "th"], recursive=False)
            if max(name_index, type_index, description_index) >= len(cells):
                continue
            name = text(cells[name_index])
            if not name:
                continue
            # OKX denotes each level of nesting by one or more > characters.
            nested = re.match(r"^(>+)\s*(.*)$", name)
            if nested:
                name = nested.group(1) + " " + nested.group(2).strip()
            requirement = text(cells[required_index]) if required_index is not None and required_index < len(cells) else ""
            description = text(cells[description_index])
            conditional = bool(
                table_number > 0
                or re.search(r"条件|视情况|根据情况|按情况", requirement)
                or re.search(r"^[^。；;:：]{0,60}仅适用", description)
                or re.search(r"必填|必须填写|必须传|必传", description)
                and requirement.lower() in {"否", "可选", "否/是", "no", "optional"}
            )
            required = requirement in {"是", "必填", "Yes", "YES", "yes", "true", "True"}
            parameters.append({"name": name, "type": text(cells[type_index]), "required": required,
                               "conditional": conditional, "description": description})
            if name.lstrip("> ") in {"channel", "args"}:
                for code in cells[description_index].find_all("code"):
                    # The official index candle list has a stray space before
                    # the hyphen; normalize that obvious family-name typo.
                    channel = re.sub(r"\s*-\s*", "-", text(code))
                    if re.fullmatch(r"[a-zA-Z0-9_-]+", channel) and channel not in channel_values:
                        channel_values.append(channel)
    return parameters, channel_values


def examples(nodes):
    """Return shell/JSON snippets identified by the docs as request examples."""
    in_request_example = False
    result = []
    for node in nodes:
        if node.name == "blockquote":
            label = text(node)
            if "请求示例" in label:
                in_request_example = True
            elif re.search(r"返回|响应|推送", label):
                in_request_example = False
        elif node.name in {"h4", "h5"} and "请求参数" in text(node):
            in_request_example = False
        elif in_request_example and node.name == "div":
            pre = node.find("pre")
            if pre and ({"shell", "json"} & set(pre.get("class", []))):
                result.append(pre.get_text())
    return result


def body_kind(snippets):
    for snippet in snippets:
        match = re.search(r"\bbody\s*([\[{])", snippet, re.IGNORECASE)
        if match:
            return "array" if match.group(1) == "[" else "object"
        stripped = snippet.lstrip()
        if stripped.startswith("["):
            return "array"
    return "object"


def rest_auth(method, path, title, nodes):
    # These documented endpoints are public.  Announcements additionally allow
    # optional private authentication; exposing the public mode keeps callers
    # from requiring credentials for the supported public request.
    if path in PUBLIC_PATHS:
        return "public"
    permission = " ".join(text(node) for node in nodes if node.name in {"h4", "h5"}
                          and re.search(r"权限|permission", text(node), re.IGNORECASE))
    if (path in {"/api/v5/asset/withdrawal", "/api/v5/asset/cancel-withdrawal"}
            or "提币" in permission or re.search(r"\bwithdraw(?:al)?\b", permission, re.IGNORECASE)):
        return "withdraw"
    if "交易" in permission or re.search(r"\btrade\b", permission, re.IGNORECASE):
        return "trade"
    if "读取" in permission or "只读" in permission or re.search(r"\bread\b", permission, re.IGNORECASE):
        return "read"
    before_request = []
    for node in nodes:
        if node.name in {"h4", "h5"} and "HTTP请求" in text(node):
            break
        if node.name in {"h4", "h5", "p", "aside"}:
            before_request.append(text(node))
    if any(re.search(r"(?<!不)(?<!无)(?:需要|必须|需).{0,6}(?:登录|鉴权|认证|签名)", value)
           for value in before_request):
        return "read" if method == "GET" else "trade"
    if ("公共" in permission or "无需" in permission or "公共" in title
            or any(re.search(r"公共接口|无需(?:登录|鉴权|认证)|免(?:登录|鉴权|认证)", value) for value in before_request)
            or path == "/api/v5/rfq/public-trades"
            or path.startswith(("/api/v5/market/", "/api/v5/public/", "/api/v5/rubik/"))):
        return "public"
    return "read" if method == "GET" else "trade"


def parse_ws_request(snippets):
    decoder = json.JSONDecoder()
    for snippet in snippets:
        # Some examples contain a short comment before the JSON object.
        for match in re.finditer(r"\{", snippet):
            candidate = snippet[match.start():]
            # One official example omits a comma between two lines of object
            # members. Only repair this unambiguous newline/key boundary; never
            # invent values or silently accept other malformed JSON.
            for _ in range(4):
                try:
                    value, _ = decoder.raw_decode(candidate)
                    break
                except json.JSONDecodeError as error:
                    prefix = candidate[:error.pos]
                    if (error.msg != "Expecting ',' delimiter" or error.pos >= len(candidate)
                            or candidate[error.pos] != '"' or not prefix.rstrip().endswith(('"', '}', ']'))
                            or "\n" not in prefix[len(prefix.rstrip()):]):
                        value = None
                        break
                    candidate = prefix + "," + candidate[error.pos:]
            else:
                value = None
            if isinstance(value, dict) and isinstance(value.get("op"), str) and isinstance(value.get("args"), list):
                return value["op"], [arg for arg in value["args"] if isinstance(arg, dict)]
    return "", []


def sbe_websocket(soup):
    """Extract the separately documented binary WebSocket service, if present."""
    group = soup.find(id="order-book-trading-sbe-market-data")
    if not group:
        return None
    sections = {}
    for node in group.next_siblings:
        if getattr(node, "name", None) in {"h1", "h2"}:
            break
        if getattr(node, "name", None) == "h3":
            sections[text(node)] = node
    overview, general, integration = (sections.get(name) for name in ("概述", "基本信息", "接入信息"))
    if not all((overview, general, integration)):
        raise ValueError("incomplete SBE WebSocket documentation")
    channels = []
    for node in section_nodes(overview):
        for code in node.find_all("code"):
            channel = text(code)
            if re.fullmatch(r"[a-zA-Z0-9_-]+", channel) and channel not in channels:
                channels.append(channel)
    general_nodes = list(section_nodes(general))
    if "/ws/v5/public-sbe" not in " ".join(text(node) for node in general_nodes):
        raise ValueError("SBE WebSocket service URL missing")
    first_bullet = next((node.find("li") for node in general_nodes if node.name == "ul" and node.find("li")), None)
    permissions = text(first_bullet) if first_bullet else ""
    snippets = [pre.get_text() for node in section_nodes(integration) for pre in node.find_all("pre")]
    operation, sample_args = parse_ws_request(snippets)
    if not channels or operation != "subscribe" or not sample_args:
        raise ValueError("SBE WebSocket channels or subscription example missing")
    parent = group.find_previous("h1")
    return {
        "id": "order-book-trading-sbe-market-data-websocket",
        "category": text(parent) if parent else "", "group": text(group),
        "title": "SBE 二进制行情（需要登录）", "service": "public-sbe", "requires_login": True,
        "operation": operation, "channels": channels,
        "parameters": [
            {"name": "id", "type": "String", "required": False, "conditional": False, "description": "消息的唯一标识。"},
            {"name": "op", "type": "String", "required": True, "conditional": False, "description": "操作：subscribe。"},
            {"name": "args", "type": "Array of objects", "required": True, "conditional": False, "description": "请求订阅的频道列表。"},
            {"name": "> channel", "type": "String", "required": True, "conditional": False,
             "description": "SBE 二进制行情。" + permissions},
            {"name": "> instIdCode", "type": "Integer", "required": True, "conditional": False,
             "description": "产品 ID 唯一标识码。"},
        ],
        "sample_args": sample_args, "doc_url": SOURCE_URL + "#" + integration["id"], "mutating": False,
    }


def build_catalogue(html, retrieved_on):
    soup = BeautifulSoup(html, "html.parser")
    rest, websocket = [], []
    timeout_section = soup.find(id="overview-transaction-timeouts-rest-api")
    timeout_anchors = set()
    if timeout_section:
        for node in section_nodes(timeout_section):
            if node.name == "ul":
                timeout_anchors.update(link["href"].split("#", 1)[1] for link in node.find_all("a", href=True)
                                       if "#" in link["href"])
    category = group = ""
    for heading in soup.find_all(["h1", "h2", "h3"]):
        if heading.name == "h1":
            category, group = text(heading), ""
            continue
        if heading.name == "h2":
            # Most h2 headings name a group, but the official page also uses
            # h2 directly for a few REST interfaces.  Inspect only this
            # heading's section: a route in a following h3 must not promote
            # this h2 group into an endpoint.
            h2_nodes = list(section_nodes(heading))
            h2_has_route = any(
                node.name == "p" and node.find("code") and ROUTE.fullmatch(text(node))
                for node in h2_nodes
            )
            if not h2_has_route:
                group = text(heading)
                continue
            # An endpoint-shaped h2 is a peer of the current h1, not a child
            # of the preceding h2 group (or of the preceding endpoint-shaped
            # h2).
            group = ""
        anchor = heading.get("id")
        if not anchor:
            continue
        nodes = list(section_nodes(heading))
        routes = []
        for node in nodes:
            if node.name != "p" or not node.find("code"):
                continue
            match = ROUTE.fullmatch(text(node))
            if match:
                routes.append((match.group(1).upper(), match.group(2)))
        services = [match.group(1).lower() for node in nodes if node.name == "p" for match in WS_SERVICE.finditer(text(node))]
        if not routes and not services:
            continue
        parameters, channel_values = extract_parameters(nodes)
        snippets = examples(nodes)
        title = text(heading)
        doc_url = SOURCE_URL + "#" + anchor
        for method, path in routes:
            rate_limit = "；".join(text(node) for node in nodes if node.name in {"h4", "h5"} and "限速" in text(node))
            auth = rest_auth(method, path, title, nodes)
            public_query = auth == "public" and bool(re.search(r"查询|计算|估算|预估|回测", title))
            rest.append({"id": anchor, "category": category, "group": group, "title": title, "method": method,
                         "path": path, "auth": auth, "mutating": method != "GET" and not public_query,
                         "body_kind": body_kind(snippets), "parameters": parameters, "doc_url": doc_url,
                         "rate_limit": rate_limit,
                         "supports_exp_time": method == "POST" and anchor in timeout_anchors,
                         "already_integrated": path in (INTEGRATED_GET if method == "GET" else INTEGRATED_POST)})
        if services and "sbe" not in (anchor + " " + title).lower():
            operation, sample_args = parse_ws_request(snippets)
            if not operation or operation in {"login", "unsubscribe"}:
                continue
            for arg in sample_args:
                channel = arg.get("channel")
                if isinstance(channel, str) and channel not in channel_values:
                    channel_values.append(channel)
            if anchor == "order-book-trading-market-data-ws-order-book-channel":
                # The docs list books-rpi and deprecated books-elp in the
                # introductory bullets, although their parameter table omits them.
                for node in nodes:
                    if node.name != "ul":
                        continue
                    for bullet in node.find_all("li", recursive=False):
                        code = bullet.find("code")
                        if code and text(code).startswith("books") and text(bullet).startswith(text(code)):
                            channel = text(code)
                            if channel not in channel_values:
                                channel_values.append(channel)
            if any("sbe" in channel.lower() for channel in channel_values):
                continue
            service = services[0]
            service_text = " ".join(text(node) for node in nodes if node.name == "p" and WS_SERVICE.search(text(node)))
            requires_login = service == "private" or bool(re.search(r"(?<!不)(?<!无)需要登录|需登录|需要鉴权", service_text))
            entry = {"id": anchor, "category": category, "group": group, "title": title,
                     "service": service, "requires_login": requires_login, "operation": operation,
                     "channels": channel_values, "parameters": parameters, "sample_args": sample_args,
                     "doc_url": doc_url, "mutating": operation != "subscribe"}
            if anchor == "order-book-trading-market-data-ws-order-book-channel":
                vip_channels = [channel for channel in channel_values if channel in {"books-l2-tbt", "books50-l2-tbt"}]
                entry["channels"] = [channel for channel in channel_values if channel not in vip_channels]
                if vip_channels:
                    vip_args = [dict(sample_args[0], channel=vip_channels[0])] if sample_args else []
                    websocket.append({**entry, "id": anchor + "-vip", "title": title + "（VIP4+）",
                                      "requires_login": True, "channels": vip_channels, "sample_args": vip_args})
            websocket.append(entry)
    sbe = sbe_websocket(soup)
    if sbe:
        websocket.append(sbe)
    for label, records in (("REST", rest), ("WebSocket", websocket)):
        ids = [record["id"] for record in records]
        if len(ids) != len(set(ids)):
            raise ValueError(f"duplicate {label} endpoint anchor")
    return {"source_url": SOURCE_URL, "retrieved_on": retrieved_on, "rest": rest, "websocket": websocket}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--input", required=True, type=Path, help="downloaded official HTML")
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--retrieved-on", required=True, type=dt.date.fromisoformat, help="snapshot date YYYY-MM-DD")
    args = parser.parse_args()
    catalogue = build_catalogue(args.input.read_text(encoding="utf-8"), args.retrieved_on.isoformat())
    if not catalogue["rest"] and not catalogue["websocket"]:
        parser.error("no API sections found in input")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(catalogue, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"Wrote {len(catalogue['rest'])} REST and {len(catalogue['websocket'])} WebSocket entries to {args.output}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        sys.exit(1)
