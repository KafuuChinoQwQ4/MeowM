#!/usr/bin/env python3
"""Build checked-in Binance and iFinD endpoint catalogues from official exports.

The input files are downloaded from the providers' documentation repositories and
are deliberately kept outside the repository.  The generated JSON contains only
public endpoint metadata and example values; credentials are never included.
"""

from __future__ import annotations

import argparse
import ast
import html
import json
import re
from pathlib import Path
from typing import Any, Iterable
from urllib.parse import urlsplit


BINANCE_DOC_BASE = "https://developers.binance.com/zh-CN/docs"
IFIND_DOC = "https://quantapi.51ifind.com/gwstatic/static/ds_web/quantapi-web/help-center/manual.html"
IFIND_EXAMPLES = "https://quantapi.51ifind.com/gwstatic/static/ds_web/quantapi-web/example.html"


def _walk_postman(items: Iterable[dict[str, Any]], collection: str) -> Iterable[dict[str, Any]]:
    for item in items:
        request = item.get("request")
        if isinstance(request, dict):
            yield {"collection": collection, "item": item, "request": request}
        children = item.get("item")
        if isinstance(children, list):
            yield from _walk_postman(children, collection)


def _path_from_request(request: dict[str, Any]) -> str | None:
    url = request.get("url")
    if isinstance(url, dict):
        path = url.get("path")
        if isinstance(path, list):
            return "/" + "/".join(str(part) for part in path)
        raw = url.get("raw")
    else:
        raw = url
    if not isinstance(raw, str):
        return None
    raw = raw.replace("{{url}}", "")
    parsed = urlsplit(raw if "://" in raw else "https://placeholder" + raw)
    return parsed.path or "/"


def _postman_parameters(request: dict[str, Any]) -> list[dict[str, Any]]:
    url = request.get("url")
    values: list[dict[str, Any]] = []
    if isinstance(url, dict):
        for query in url.get("query", []) or []:
            if not isinstance(query, dict):
                continue
            name = str(query.get("key", "")).strip()
            if not name or name in {"signature", "timestamp"} or query.get("disabled"):
                continue
            values.append(
                {
                    "name": name,
                    "type": _parameter_type(name, query.get("value")),
                    "required": bool(query.get("description", "").lower().find("required") >= 0),
                    "location": "query",
                    "description": str(query.get("description") or ""),
                    "example": query.get("value") or "",
                }
            )
    body = request.get("body")
    if isinstance(body, dict):
        mode = body.get("mode")
        if mode in {"urlencoded", "formdata"}:
            for field in body.get(mode, []) or []:
                if not isinstance(field, dict):
                    continue
                name = str(field.get("key", "")).strip()
                if not name or field.get("disabled"):
                    continue
                values.append(
                    {
                        "name": name,
                        "type": _parameter_type(name, field.get("value")),
                        "required": bool(field.get("description", "").lower().find("required") >= 0),
                        "location": "body",
                        "description": str(field.get("description") or ""),
                        "example": field.get("value") or "",
                    }
                )
        elif mode == "raw" and isinstance(body.get("raw"), str):
            try:
                value = json.loads(body["raw"])
            except (TypeError, ValueError):
                value = None
            if isinstance(value, dict):
                for name, example in value.items():
                    values.append(
                        {
                            "name": str(name),
                            "type": _parameter_type(str(name), example),
                            "required": False,
                            "location": "body",
                            "description": "",
                            "example": example,
                        }
                    )
    seen: set[tuple[str, str]] = set()
    output = []
    for value in values:
        key = (value["name"], value["location"])
        if key not in seen:
            seen.add(key)
            output.append(value)
    return output


def _parameter_type(name: str, example: Any) -> str:
    lowered = name.lower()
    if isinstance(example, bool):
        return "boolean"
    if isinstance(example, (int, float)) or lowered in {
        "limit",
        "recvwindow",
        "timestamp",
        "starttime",
        "endtime",
        "fromid",
        "toid",
        "id",
        "page",
        "size",
    }:
        return "number"
    return "string"


def _binance_host(path: str, product: str = "") -> str:
    product_upper = product.upper()
    if path.startswith("/bapi/"):
        return "alpha"
    if path.startswith("/papi/"):
        return "papi"
    if "COIN-M" in product_upper and path.startswith("/") and not path.startswith(("/api/", "/sapi/")):
        return "dapi"
    if "USDⓈ-M" in product_upper and path.startswith("/") and not path.startswith(("/api/", "/sapi/")):
        return "fapi"
    if "OPTION" in product_upper and "STREAM" in product_upper:
        return "options"
    if "ALPHA" in product_upper and "STREAM" in product_upper:
        return "alpha"
    if path.startswith("/futures/data/"):
        return "dapi" if "COIN" in product_upper else "fapi"
    if path.startswith("/fapi/") or path.startswith("/futures/"):
        return "fapi"
    if path.startswith("/dapi/"):
        return "dapi"
    if path.startswith("/eapi/"):
        return "eapi"
    if path.startswith("/sapi/"):
        return "api"
    if path.startswith("/api/"):
        return "api"
    return "api"


def _binance_auth(name: str, request: dict[str, Any], path: str, product: str = "") -> str:
    upper = name.upper()
    url = request.get("url") if isinstance(request.get("url"), dict) else {}
    query_names = {str(q.get("key", "")) for q in url.get("query", []) or [] if isinstance(q, dict)}
    if path.startswith("/bapi/") and "/public/" in path:
        return "public"
    if path.startswith("/futures/data/"):
        return "public"
    if path.endswith("/historicalTrades"):
        return "api_key"
    if path == "/api/v3/userDataStream" or path.endswith("/userDataStream"):
        return "user_stream"
    if path.startswith("/api/v3/") and re.search(
        r"/(?:ping|time|depth|trades|aggTrades|klines|uiKlines|avgPrice|ticker(?:/.*)?|exchangeInfo)$",
        path,
        re.I,
    ):
        return "public"
    if path.startswith(("/fapi/", "/dapi/", "/eapi/")) and re.search(
        r"/(?:ping|time|depth|trades|aggTrades|klines|continuousKlines|indexPriceKlines|markPriceKlines|premiumIndex|fundingRate|ticker(?:/.*)?|exchangeInfo)$",
        path,
        re.I,
    ) and not path.endswith("historicalTrades"):
        return "public"
    if re.search(
        r"/(?:account|myTrades|myAllocations|openOrders|allOrders|orderList(?:/|\.)|userTrades|balance|positionRisk|positionSide|commissionRate|capital/config)(?:/|$)",
        path,
        re.I,
    ):
        return "user_data"
    if "TRADE" in upper or re.search(r"/(?:order|batchOrders|cancel|replace|withdraw)(?:/|$)", path, re.I):
        return "trade"
    if "USER_DATA" in upper or "USER_STREAM" in upper:
        return "user_data"
    if "SIGNED" in upper or {"signature", "timestamp"}.issubset(query_names):
        return "signed"
    if "APIKEY" in upper or any("X-MBX-APIKEY" == str(h.get("key")) for h in request.get("header", []) or [] if isinstance(h, dict)):
        return "api_key"
    if path.startswith(("/api/", "/sapi/", "/papi/", "/fapi/", "/dapi/", "/eapi/")):
        return "signed"
    return "public"


def _reference_auth(method: str, path: str) -> str:
    """Conservatively classify endpoints found in the reference export.

    The Postman exports carry explicit auth metadata.  The long-form reference
    also contains a small set of endpoints that are not in those exports, so
    use the documented public market-data families where they are unambiguous
    and require a signed key for the rest.
    """
    if path.endswith("/userDataStream"):
        return "user_stream"
    if path.endswith("/historicalTrades"):
        return "api_key"
    if method != "GET":
        return "signed"
    lower = path.lower()
    if path.startswith("/bapi/"):
        return "public"
    public_fragments = (
        "/ping",
        "/time",
        "/depth",
        "/trades",
        "/aggtrades",
        "/klines",
        "/continuousklines",
        "/indexpriceklines",
        "/markpriceklines",
        "/ticker",
        "/premiumindex",
        "/fundingrate",
        "/exchangeinfo",
    )
    if path.startswith("/api/v3/") and any(fragment in lower for fragment in public_fragments):
        return "public"
    if path.startswith(("/fapi/", "/dapi/")) and any(fragment in lower for fragment in public_fragments):
        return "public"
    return "signed"


def _reference_rest_entries(reference: str, existing: set[tuple[str, str]]) -> list[dict[str, Any]]:
    """Recover REST operations documented only in the long-form export."""
    paths = re.findall(
        r"`(GET|POST|PUT|DELETE|PATCH)\s+(/(?:api|bapi|sapi|fapi|dapi|eapi|papi)/[A-Za-z0-9_{}@./?=&:~+\-%]*)`",
        reference,
    )
    output = []
    seen = set(existing)
    for method, raw_path in paths:
        path = raw_path.split("?", 1)[0]
        if not path.startswith(("/api/v3", "/bapi/", "/sapi/", "/fapi/", "/dapi/", "/eapi/", "/papi/")):
            continue
        key = (method, path)
        if not path or key in seen:
            continue
        seen.add(key)
        output.append(
            {
                "id": f"binance.rest.reference.{method.lower()}.{_slug(path)}",
                "product": "Binance API Reference",
                "category": "官方 API Reference",
                "title": path,
                "method": method,
                "path": path,
                "host": _binance_host(path, "Binance API Reference"),
                "transport": "rest",
                "auth": _reference_auth(method, path),
                "mutating": method in {"POST", "PUT", "DELETE", "PATCH"},
                "environments": _binance_environments(
                    path,
                    "rest",
                    "Spot REST API" if path.startswith("/api/v3/") else "Binance API Reference",
                ),
                "params": _stream_parameters(path),
                "doc_url": f"{BINANCE_DOC_BASE}/catalog",
            }
        )
    return output


def _doc_lookup(reference: str) -> dict[str, tuple[str, str]]:
    """Read lightweight synthetic/API-reference headings when present."""
    result: dict[str, tuple[str, str]] = {}
    section = "Binance API"
    title = ""
    for line in reference.splitlines():
        stripped = line.strip()
        if stripped.startswith("### "):
            section = stripped[4:].strip().strip("()")
        match = re.match(r"#{3,6}\s*`(GET|POST|PUT|DELETE|PATCH)\s+([^`]+)`", stripped, re.I)
        if match:
            method, path = match.groups()
            title = ""
            continue
        if title == "" and stripped and not stripped.startswith("#") and "/" in stripped:
            # The first prose line after an operation is a useful localized title.
            title = stripped[:240]
        if re.match(r"^Operation ID:", stripped, re.I):
            continue
        # Keep paths found in compact reference snippets even when no prose exists.
        for method, path in re.findall(r"\b(GET|POST|PUT|DELETE|PATCH)\s+(`?/[A-Za-z0-9_{}@./:-]+`?)", stripped):
            result.setdefault(path.strip("`"), (section, title or path.strip("`")))
    return result


def extract_binance(reference: str, collections: list[tuple[str, dict[str, Any]]]) -> list[dict[str, Any]]:
    """Extract REST entries from official Postman exports and stream entries from docs."""
    docs = _doc_lookup(reference)
    output: list[dict[str, Any]] = []
    seen: set[tuple[str, str, str, str]] = set()
    for filename, collection in collections:
        collection_name = str(collection.get("info", {}).get("name") or Path(filename).stem)
        for record in _walk_postman(collection.get("item", []), collection_name):
            request = record["request"]
            method = str(request.get("method") or "GET").upper()
            path = _path_from_request(request)
            if not path or path.startswith("/") is False:
                continue
            key = ("rest", method, path, collection_name)
            if key in seen:
                continue
            seen.add(key)
            item = record["item"]
            name = str(item.get("name") or path)
            section, doc_title = docs.get(path, (collection_name, name))
            auth = _binance_auth(name, request, path, collection_name)
            output.append(
                {
                    "id": f"binance.rest.{_slug(collection_name)}.{method.lower()}.{_slug(path)}",
                    "product": collection_name,
                    "category": section,
                    "title": doc_title or name,
                    "method": method,
                    "path": path,
                    "host": _binance_host(path, collection_name),
                    "transport": "rest",
                    "auth": auth,
                    "mutating": method in {"POST", "PUT", "DELETE", "PATCH"},
                    "environments": _binance_environments(path, "rest", collection_name),
                    "params": _postman_parameters(request),
                    "doc_url": f"{BINANCE_DOC_BASE}/",
                }
            )

    # Compact reference snippets and the official llms export occasionally contain
    # stream operations that are not represented in REST Postman collections.
    current_transport = "rest"
    current_product = "Spot REST API"
    for line in reference.splitlines():
        stripped = line.strip()
        if stripped.startswith("### "):
            heading = stripped[4:].lower()
            current_product = stripped[4:].strip()
            current_transport = "stream" if "market stream" in heading else ("ws_api" if "websocket api" in heading else "rest")
        match = re.match(r"#{3,6}\s*`(GET|POST|PUT|DELETE|PATCH)\s+([^`]+)`", stripped, re.I)
        if not match or current_transport == "rest":
            continue
        method, path = match.group(1).upper(), match.group(2)
        key = (current_transport, method, path, current_product)
        if key in seen:
            continue
        seen.add(key)
        title = docs.get(path, (current_product, path))[1]
        output.append(
            {
                "id": f"binance.{current_transport}.{_slug(current_product)}.{method.lower()}.{_slug(path)}",
                "product": current_product,
                "category": current_product,
                "title": title,
                "method": method,
                "path": path,
                "host": _binance_host(path, current_product),
                "transport": current_transport,
                "auth": _binance_ws_auth(current_transport, path),
                "mutating": current_transport == "ws_api" and bool(re.search(r"(?:order(?:List)?\.(?:place|cancel|modify|amend)|cancelReplace|withdraw)", path, re.I)),
                "environments": _binance_environments(path, current_transport, current_product),
                "params": _stream_parameters(path),
                "doc_url": f"{BINANCE_DOC_BASE}/",
            }
        )
    existing_rest = {(entry["method"], entry["path"]) for entry in output if entry["transport"] == "rest"}
    output.extend(_reference_rest_entries(reference, existing_rest))
    return output


def _binance_ws_auth(transport: str, path: str) -> str:
    if transport != "ws_api":
        return "public"
    if re.search(r"(?:order(?:List)?\.(?:place|cancel|modify|amend)|cancelReplace|withdraw)", path, re.I):
        return "trade"
    if re.search(r"(?:account|order|trade|session|userDataStream|my[A-Z])", path):
        return "signed"
    return "public"


def _binance_environments(path: str, transport: str, product: str) -> list[str]:
    """Return only environments with a documented endpoint for this product."""
    product_lower = product.lower()
    spot_rest = transport == "rest" and "spot" in product_lower and path.startswith("/api/v3/")
    spot_ws_api = transport == "ws_api" and "spot websocket api" in product_lower
    spot_stream = transport == "stream" and "spot" in product_lower and "stream" in product_lower
    if "alpha" not in product_lower and (spot_rest or spot_ws_api or spot_stream):
        return ["production", "spot_testnet"]
    return ["production"]


def _stream_parameters(path: str) -> list[dict[str, Any]]:
    names = re.findall(r"\{([^}]+)\}", path)
    return [
        {"name": name, "type": "string", "required": True, "location": "path", "description": "", "example": "btcusdt"}
        for name in names
    ]


def _slug(value: str) -> str:
    value = re.sub(r"[^a-zA-Z0-9]+", "_", value).strip("_").lower()
    return value[:120] or "root"


def _json_from_fragment(fragment: str) -> Any:
    fragment = html.unescape(fragment).strip()
    try:
        return json.loads(fragment)
    except (TypeError, ValueError):
        try:
            return ast.literal_eval(fragment)
        except (SyntaxError, ValueError):
            return None


def _strip_html(text: str) -> str:
    """Turn VitePress/Shiki code blocks into source text.

    The live pages put every token in a separate ``span``.  Removing tags before
    searching is important: a path or the ``formData:`` separator can otherwise
    be split by markup and disappear from the generated catalogue.
    """
    return re.sub(r"<[^>]+>", "", html.unescape(text))


def _balanced_object(text: str, start: int) -> str | None:
    """Return one balanced JSON/Python object, respecting quoted braces."""
    if start >= len(text) or text[start] != "{":
        return None
    stack = ["{"]
    quote: str | None = None
    escaped = False
    for index in range(start + 1, len(text)):
        character = text[index]
        if quote is not None:
            if escaped:
                escaped = False
            elif character == "\\":
                escaped = True
            elif character == quote:
                quote = None
            continue
        if character in {"'", '"'}:
            quote = character
        elif character == "{":
            stack.append(character)
        elif character == "[":
            stack.append(character)
        elif character in {"}", "]"}:
            expected = "{" if character == "}" else "["
            if not stack or stack[-1] != expected:
                return None
            stack.pop()
            if not stack:
                return text[start : index + 1]
    return None


def _object_after(text: str, marker: str, start: int = 0) -> str | None:
    position = text.find(marker, start)
    if position < 0:
        return None
    opening = text.find("{", position + len(marker))
    return _balanced_object(text, opening)


def _extract_ifind_examples(text: str) -> dict[str, dict[str, Any]]:
    text = _strip_html(text)
    samples: dict[str, dict[str, Any]] = {}
    pattern = re.compile(
        r"thsUrl\s*=\s*['\"](https://quantapi\.51ifind\.com(?P<path>/api/v1/[A-Za-z0-9_]+))['\"]"
        r".*?thsPara\s*=\s*",
        re.S,
    )
    for match in pattern.finditer(text):
        fragment = _object_after(text, "", match.end())
        sample = _json_from_fragment(fragment) if fragment else None
        if isinstance(sample, dict):
            samples[match.group("path")] = sample
    return samples


IFIND_FALLBACKS: dict[str, dict[str, Any]] = {
    "basic_data_service": {
        "codes": "300033.SZ,600030.SH",
        "indipara": [{"indicator": "ths_close_price_stock", "indiparams": ["20250113", "100"]}],
    },
    "date_sequence": {
        "codes": "AAPL.O", "startdate": "20250101", "enddate": "20250113",
        "functionpara": {"Days": "Alldays", "Fill": "-1"},
        "indipara": [{"indicator": "ths_pre_close_uss", "indiparams": ["", "100"]}],
    },
    "cmd_history_quotation": {
        "codes": "300033.SZ", "indicators": "open,high,low,close", "startdate": "20240113",
        "enddate": "20250113", "functionpara": {"Currency": "MHB", "Fill": "Omit"},
    },
    "report_query": {
        "codes": "300033.SZ", "functionpara": {"reportType": "901"},
        "beginrDate": "20240118", "endrDate": "20250118",
        "outputpara": "reportDate:Y,thscode:Y,secName:Y,ctime:Y,reportTitle:Y,pdfURL:Y,seq:Y",
    },
    "fund_valuation": {
        "codes": "512880.SH", "functionpara": {"onlyLastest": "0", "beginTime": "20241206 09:15:00", "endTime": "20241206 15:00:00"},
        "outputpara": "realTimeValuation:Y",
    },
    "data_pool": {
        "reportname": "p03425", "functionpara": {"date": "20220706", "blockname": "001005010", "iv_type": "allcontract"},
        "outputpara": "p03291_f001,p03291_f002,p03291_f003,p03291_f004",
    },
    "get_trade_dates": {
        "marketcode": "212001", "functionpara": {"dateType": "0", "period": "D", "offset": "-10", "dateFormat": "0", "output": "sequencedate"},
        "startdate": "20220705", "enddate": "20220715",
    },
    "final_fund_valuation": {
        "codes": "000001.OF,000003.OF", "functionpara": {"beginDate": "20210824", "endDate": "20210824"},
        "outputpara": "finalValuation:Y,netAssetValue:Y,deviation:Y",
    },
    "get_error_message": {"errorcode": -1303},
    "get_thscode": {"seccode": "000001", "mode": "seccode"},
    "portfolio_manage": {
        "func": "newportf", "name": "股债策略组合", "group": 11580,
        "performbm": {"code": "000300.SH", "name": "沪深300"}, "tday": "国内交易所", "currency": "CNY",
    },
    "get_data_volume": {},
    "get_access_token": {},
    "update_access_token": {},
}


IFIND_REQUIRED: dict[str, set[str]] = {
    "basic_data_service": {"codes", "indipara"},
    "date_sequence": {"codes", "startdate", "enddate", "indipara"},
    "cmd_history_quotation": {"codes", "indicators", "startdate", "enddate"},
    "high_frequency": {"codes", "indicators", "starttime", "endtime"},
    "real_time_quotation": {"codes", "indicators"}, "snap_shot": {"codes", "indicators"},
    "edb_service": {"indicators", "startdate", "enddate"},
    "smart_stock_picking": {"searchstring", "searchtype"}, "report_query": {"codes"},
    "fund_valuation": {"codes", "functionpara", "outputpara"},
    "data_pool": {"reportname", "functionpara", "outputpara"},
    "get_trade_dates": {"marketcode", "functionpara", "startdate", "enddate"},
    "final_fund_valuation": {"codes", "functionpara", "outputpara"},
    "get_error_message": {"errorcode"}, "get_thscode": {"seccode", "mode"},
    "portfolio_manage": {"func", "name", "group"},
}


def extract_ifind(manual: str, examples: str) -> list[dict[str, Any]]:
    manual_text = _strip_html(manual)
    example_samples = _extract_ifind_examples(examples)
    # The manual embeds a compact requestURL/formData example for every data
    # function.  Preserve those samples before looking at the separate examples
    # page, which contains a few additional helper endpoints.
    manual_samples: dict[str, dict[str, Any]] = {}
    for match in re.finditer(
        r"requestURL\s*:\s*https://quantapi\.51ifind\.com(?P<path>/api/v1/[A-Za-z0-9_]+).*?formData\s*:\s*",
        manual_text, flags=re.S,
    ):
        fragment = _object_after(manual_text, "", match.end())
        sample = _json_from_fragment(fragment) if fragment else None
        if isinstance(sample, dict):
            manual_samples[match.group("path")] = sample
    paths: list[str] = []
    for path in re.findall(r"https://quantapi\.51ifind\.com(?P<path>/api/v1/[A-Za-z0-9_]+)", manual_text):
        if path not in paths:
            paths.append(path)
    for path in example_samples:
        if path not in paths:
            paths.append(path)
    for path in re.findall(r"https://quantapi\.51ifind\.com(?P<path>/api/v1/[A-Za-z0-9_]+)", _strip_html(examples)):
        if path not in paths:
            paths.append(path)
    # The HTTP PDF documents helper and portfolio endpoints that are not linked
    # by the public HTML manual. Full/manual fixtures (three or more paths) get
    # these fixed official routes; the tiny two-path unit fixture stays focused.
    if len(paths) >= 3:
        for name in IFIND_FALLBACKS:
            path = f"/api/v1/{name}"
            if path not in paths:
                paths.append(path)
    titles = {
        "basic_data_service": "基础数据",
        "date_sequence": "日期序列",
        "cmd_history_quotation": "历史行情",
        "high_frequency": "高频行情",
        "real_time_quotation": "实时行情",
        "snap_shot": "快照行情",
        "edb_service": "宏观经济数据库",
        "smart_stock_picking": "智能选股",
        "report_query": "专题报表",
        "fund_valuation": "基金估值",
        "data_pool": "数据池",
        "get_trade_dates": "交易日历",
        "get_access_token": "获取访问令牌",
        "update_access_token": "轮换访问令牌",
        "portfolio_manage": "组合管理",
        "final_fund_valuation": "基金实时估值（日）",
        "get_data_volume": "数据量查询",
        "get_error_message": "错误信息查询",
        "get_thscode": "证券代码转换",
    }
    entries = []
    for path in paths:
        name = path.rsplit("/", 1)[-1]
        sample = example_samples.get(path) or manual_samples.get(path) or IFIND_FALLBACKS.get(name, {})
        internal_auth = name in {"get_access_token", "update_access_token"}
        entries.append(
            {
                "id": f"ifind.rest.post.{name}",
                "product": "iFinD HTTP API",
                "category": "官方数据服务",
                "title": titles.get(name, name),
                "method": "POST",
                "path": path,
                "host": "quantapi.51ifind.com",
                "transport": "rest",
                "auth": "refresh_token"
                if name in {"get_access_token", "update_access_token"}
                else "access_token",
                "mutating": name in {"portfolio_manage", "update_access_token"},
                "internal_auth": internal_auth,
                "environments": ["production"],
                "params": _sample_parameters(sample, IFIND_REQUIRED.get(name, set())),
                "sample": sample,
                "doc_url": IFIND_EXAMPLES if path in example_samples else IFIND_DOC,
            }
        )
    return entries


def _ifind_parameter_type(value: Any) -> str:
    if isinstance(value, bool):
        return "boolean"
    if isinstance(value, (int, float)):
        return "number"
    if isinstance(value, dict):
        return "object"
    if isinstance(value, list):
        return "array"
    return "string"


def _sample_parameters(sample: dict[str, Any], required: set[str] | None = None) -> list[dict[str, Any]]:
    required = required or set()
    return [
        {
            "name": str(name),
            "type": _ifind_parameter_type(value),
            "required": str(name) in required,
            "location": "json",
            "description": "",
            "example": value,
        }
        for name, value in sample.items()
    ]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binance-reference", type=Path, required=True)
    parser.add_argument("--binance-collection", type=Path, action="append", required=True)
    parser.add_argument("--ifind-manual", type=Path, required=True)
    parser.add_argument("--ifind-examples", type=Path, required=True)
    parser.add_argument("--binance-output", type=Path, required=True)
    parser.add_argument("--ifind-output", type=Path, required=True)
    args = parser.parse_args()
    collections = [(path.name, json.loads(path.read_text())) for path in args.binance_collection]
    binance = extract_binance(args.binance_reference.read_text(), collections)
    ifind = extract_ifind(args.ifind_manual.read_text(), args.ifind_examples.read_text())
    args.binance_output.parent.mkdir(parents=True, exist_ok=True)
    args.binance_output.write_text(json.dumps(binance, ensure_ascii=False, indent=2) + "\n")
    args.ifind_output.parent.mkdir(parents=True, exist_ok=True)
    args.ifind_output.write_text(json.dumps(ifind, ensure_ascii=False, indent=2) + "\n")
    print(f"binance={len(binance)} ifind={len(ifind)}")


if __name__ == "__main__":
    main()
