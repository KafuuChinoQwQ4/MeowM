import assert from 'node:assert/strict';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
import { installWorkflowIpc } from './helpers/workflow-ipc.mjs';
import { navigate } from './helpers/navigation.mjs';

const { url, newPage } = browserSuite('1448');

async function open(hideAmounts = false) {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  await installWorkflowIpc(page);
  await page.addInitScript((hide) => {
    const original = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.transformCallback = () => 1;
    window.__TAURI_INTERNALS__.unregisterCallback = () => {};
    const parameter = (name, required, type = 'String') => ({
      name,
      required,
      conditional: false,
      type,
      description: `${name} 描述`,
    });
    const catalog = {
      source_url: 'https://www.okx.com/docs-v5/zh/',
      retrieved_on: '2026-11-08',
      rest: [
        {
          id: 'books',
          category: '撮合交易',
          group: '行情数据',
          title: '获取产品深度',
          method: 'GET',
          path: '/api/v5/market/books',
          auth: 'public',
          mutating: false,
          body_kind: 'object',
          parameters: [parameter('instId', true), parameter('sz', false)],
          doc_url: 'https://www.okx.com/docs-v5/zh/#order-book-trading-market-data-get-order-book',
          rate_limit: '40次/2s',
          already_integrated: false,
        },
        {
          id: 'transfer',
          category: '资金账户',
          group: 'REST API',
          title: '资金划转',
          method: 'POST',
          path: '/api/v5/asset/transfer',
          auth: 'trade',
          mutating: true,
          body_kind: 'object',
          parameters: [
            parameter('ccy', true),
            parameter('amt', true),
            parameter('from', true),
            parameter('to', true),
          ],
          doc_url: 'https://www.okx.com/docs-v5/zh/#funding-account-rest-api-funds-transfer',
          rate_limit: '2次/2s',
          already_integrated: false,
        },
        {
          id: 'batch',
          category: '撮合交易',
          group: '交易',
          title: '批量下单',
          method: 'POST',
          path: '/api/v5/trade/batch-orders',
          auth: 'trade',
          mutating: true,
          body_kind: 'array',
          parameters: [parameter('instId', true), parameter('sz', true)],
          doc_url:
            'https://www.okx.com/docs-v5/zh/#order-book-trading-trade-post-place-multiple-orders',
          rate_limit: '10次/2s',
          already_integrated: false,
        },
        {
          id: 'algo',
          category: '撮合交易',
          group: '策略',
          title: '策略委托下单',
          method: 'POST',
          path: '/api/v5/trade/order-algo',
          auth: 'trade',
          mutating: true,
          body_kind: 'object',
          parameters: [
            parameter('ordType', true),
            { ...parameter('szLimit', true), conditional: true },
            { ...parameter('szLimit', false), conditional: true },
          ],
          doc_url:
            'https://www.okx.com/docs-v5/zh/#order-book-trading-algo-trading-post-place-algo-order',
          rate_limit: '',
          already_integrated: false,
        },
        {
          id: 'books-sbe',
          category: '撮合交易',
          group: '行情数据',
          title: 'SBE 产品深度',
          method: 'GET',
          path: '/api/v5/market/books-sbe',
          auth: 'public',
          mutating: false,
          body_kind: 'object',
          parameters: [parameter('instIdCode', true, 'Integer')],
          doc_url: 'https://www.okx.com/docs-v5/zh/',
          rate_limit: '',
          already_integrated: false,
        },
      ],
      websocket: [
        {
          id: 'tickers',
          category: '撮合交易',
          group: '行情数据',
          title: '行情频道',
          service: 'public',
          requires_login: false,
          operation: 'subscribe',
          channels: ['tickers'],
          parameters: [parameter('channel', true), parameter('instId', true)],
          sample_args: [{ channel: 'tickers', instId: 'BTC-USDT' }],
          doc_url:
            'https://www.okx.com/docs-v5/zh/#order-book-trading-market-data-ws-tickers-channel',
          mutating: false,
        },
        {
          id: 'ws-order',
          category: '撮合交易',
          group: '交易',
          title: '实时下单',
          service: 'private',
          requires_login: true,
          operation: 'order',
          channels: [],
          parameters: [],
          sample_args: [{ instId: 'BTC-USDT', sz: '1' }],
          doc_url: 'https://www.okx.com/docs-v5/zh/',
          mutating: true,
        },
      ],
    };
    window.__TAURI_INTERNALS__.invoke = async (command, args) => {
      if (command === 'privacy_status' && hide)
        return { locked: false, lock_enabled: false, hide_amounts: true, auto_lock_minutes: 5 };
      if (command === 'okx_api_catalogue') return catalog;
      if (command === 'okx_api_request') {
        window.workflowCalls.push({ command, args });
        return {
          status: window.nextApiStatus ?? 'success',
          code: '0',
          message: '',
          data: [{ result: 'ok', amount: '1234.5678' }],
          request_id: args.request.request_id,
          http_status: 200,
        };
      }
      if (command === 'okx_ws_start') {
        window.workflowCalls.push({ command, args: { request: args.request } });
        window.streamEventHandler = args.onEvent.onmessage;
        if (window.closeStreamBeforeStart) {
          args.onEvent.onmessage({
            kind: 'closed',
            sequence: 1,
            data: { status: 'closed', reason: 'connect_failed' },
          });
          await new Promise((resolve) => setTimeout(resolve, 100));
          return 'stream-1';
        }
        if (args.request.endpoint_id === 'tickers')
          setTimeout(
            () =>
              args.onEvent.onmessage({
                kind: 'message',
                sequence: 1,
                data: { arg: { channel: 'tickers' }, data: [{ last: '123.45' }] },
              }),
            10,
          );
        if (window.delayStreamStart) await new Promise((resolve) => setTimeout(resolve, 200));
        return 'stream-1';
      }
      if (command === 'okx_ws_ack') {
        window.workflowCalls.push({ command, args });
        return;
      }
      if (command === 'okx_ws_stop') {
        window.workflowCalls.push({ command, args });
        return { status: 'closed', request_id: '' };
      }
      if (command === 'okx_tickers' || command === 'okx_watchlist' || command === 'okx_candles')
        return [];
      return original(command, args);
    };
  }, hideAmounts);
  await page.goto(url);
  await page.locator('.balance-value').waitFor();
  await navigate(page, 'markets');
  await page.getByRole('button', { name: '打开欧易', exact: true }).click();
  return page;
}

test('API workspace searches official endpoints and submits a public query', async () => {
  const page = await open();
  try {
    await page.getByRole('tab', { name: '更多接口', exact: true }).click();
    await page.getByLabel('搜索接口').fill('产品深度');
    await page.getByLabel('选择接口').selectOption('books');
    assert.equal(await page.getByLabel('交易环境').inputValue({ timeout: 2000 }), 'production');
    await page.getByLabel('交易环境').selectOption('demo');
    await page.getByLabel('instId', { exact: true }).fill('BTC-USDT');
    await page.getByLabel('sz', { exact: true }).fill('5');
    await page.getByRole('button', { name: '查询', exact: true }).click();
    await page.getByText(/"result": "ok"/).waitFor();
    const call = await page.evaluate(() =>
      window.workflowCalls.find((item) => item.command === 'okx_api_request'),
    );
    assert.equal(call.args.request.endpoint_id, 'books');
    assert.equal(call.args.request.environment, 'demo');
    assert.deepEqual(call.args.request.params, { instId: 'BTC-USDT', sz: '5' });
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
      true,
    );
  } finally {
    await page.close();
  }
});

test('hiding amounts also hides raw API responses', async () => {
  const page = await open(true);
  try {
    await page.getByRole('tab', { name: '更多接口', exact: true }).click();
    await page.getByLabel('搜索接口').fill('产品深度');
    await page.getByLabel('选择接口').selectOption('books');
    await page.getByLabel('instId', { exact: true }).fill('BTC-USDT');
    await page.getByRole('button', { name: '查询', exact: true }).click();
    await page.getByRole('region', { name: '接口结果' }).getByText('••••').waitFor();
    assert.equal(await page.getByText(/1234\.5678/).count(), 0);
  } finally {
    await page.close();
  }
});

test('money-moving requests require review and preserve exact decimal values', async () => {
  const page = await open();
  try {
    await page.getByRole('tab', { name: '更多接口', exact: true }).click();
    await page.getByLabel('搜索接口').fill('资金划转');
    await page.getByLabel('选择接口').selectOption('transfer');
    await page.getByLabel('交易环境').selectOption('production');
    for (const [name, value] of Object.entries({
      ccy: 'USDT',
      amt: '0.00000001',
      from: '6',
      to: '18',
    }))
      await page.getByLabel(name, { exact: true }).fill(value);
    await page.getByRole('button', { name: '预览操作', exact: true }).click();
    await page.getByRole('dialog', { name: '确认 OKX 操作' }).getByText(/实盘/).first().waitFor();
    assert.equal(
      await page.evaluate(
        () => window.workflowCalls.filter((item) => item.command === 'okx_api_request').length,
      ),
      0,
    );
    await page.getByRole('button', { name: '确认提交', exact: true }).click();
    await page.getByText(/"result": "ok"/).waitFor();
    const call = await page.evaluate(() =>
      window.workflowCalls.find((item) => item.command === 'okx_api_request'),
    );
    assert.equal(call.args.request.params.amt, '0.00000001');
    assert.equal(call.args.request.confirmed, true);
    assert.equal(call.args.request.environment, 'production');
  } finally {
    await page.close();
  }
});

test('uncertain writes remain visible after navigation until explicitly reconciled', async () => {
  const page = await open();
  try {
    await page.evaluate(() => {
      window.nextApiStatus = 'unknown';
    });
    await page.getByRole('tab', { name: '更多接口', exact: true }).click();
    await page.getByLabel('搜索接口').fill('资金划转');
    await page.getByLabel('选择接口').selectOption('transfer');
    for (const [name, value] of Object.entries({ ccy: 'USDT', amt: '1', from: '6', to: '18' }))
      await page.getByLabel(name, { exact: true }).fill(value);
    await page.getByRole('button', { name: '预览操作', exact: true }).click();
    await page.getByRole('button', { name: '确认提交', exact: true }).click();
    await page.getByRole('region', { name: '待核对操作' }).getByText('资金划转').waitFor();
    await page.getByRole('tab', { name: 'OKX 行情', exact: true }).click();
    await page.getByRole('tab', { name: '更多接口', exact: true }).click();
    await page.getByRole('region', { name: '待核对操作' }).getByText('资金划转').waitFor();
    await page.getByRole('button', { name: '我已核对', exact: true }).click();
    assert.equal(await page.getByRole('region', { name: '待核对操作' }).count(), 0);
  } finally {
    await page.close();
  }
});

test('WebSocket workspace displays updates and closes subscriptions on leaving', async () => {
  const page = await open();
  try {
    await page.getByRole('tab', { name: '实时频道', exact: true }).click();
    await page.getByLabel('选择频道').selectOption('tickers');
    assert.equal(await page.getByLabel('交易环境').inputValue({ timeout: 2000 }), 'production');
    await page.getByRole('button', { name: '开始订阅', exact: true }).click();
    await page.getByText(/123.45/).waitFor();
    await page.waitForFunction(() =>
      window.workflowCalls.some(
        (item) => item.command === 'okx_ws_ack' && item.args.sequence === 1,
      ),
    );
    await page.getByRole('tab', { name: 'OKX 行情', exact: true }).click();
    await page.waitForFunction(() =>
      window.workflowCalls.some((item) => item.command === 'okx_ws_stop'),
    );
    const calls = await page.evaluate(() => window.workflowCalls);
    assert.equal(
      calls.find((item) => item.command === 'okx_ws_start').args.request.endpoint_id,
      'tickers',
    );
    assert.equal(
      calls.find((item) => item.command === 'okx_ws_start').args.request.environment,
      'production',
    );
    assert.equal(calls.find((item) => item.command === 'okx_ws_stop').args.sessionId, 'stream-1');
  } finally {
    await page.close();
  }
});

test('a stream that closes before startup returns remains available to retry', async () => {
  const page = await open();
  try {
    await page.evaluate(() => {
      window.closeStreamBeforeStart = true;
    });
    await page.getByRole('tab', { name: '实时频道', exact: true }).click();
    await page.getByLabel('选择频道').selectOption('tickers');
    await page.getByRole('button', { name: '开始订阅', exact: true }).click();
    await page.getByText(/connect_failed/).waitFor();
    await page.waitForTimeout(200);
    assert.equal(
      await page.getByRole('button', { name: '开始订阅', exact: true }).isEnabled(),
      true,
    );
    assert.equal(await page.getByRole('button', { name: '断开连接', exact: true }).count(), 0);
  } finally {
    await page.close();
  }
});

test('batch order review retains JSON array and exact amounts', async () => {
  const page = await open();
  try {
    await page.getByRole('tab', { name: '更多接口', exact: true }).click();
    await page.getByLabel('搜索接口').fill('批量下单');
    await page.getByLabel('选择接口').selectOption('batch');
    await page.getByLabel('批量参数 JSON').fill('[{"instId":"BTC-USDT","sz":"0.00000001"}]');
    await page.getByRole('button', { name: '预览操作', exact: true }).click();
    await page.getByRole('button', { name: '确认提交', exact: true }).click();
    await page.getByText(/"result": "ok"/).waitFor();
    const call = await page.evaluate(() =>
      window.workflowCalls.find((item) => item.command === 'okx_api_request'),
    );
    assert.deepEqual(call.args.request.params, [{ instId: 'BTC-USDT', sz: '0.00000001' }]);
  } finally {
    await page.close();
  }
});

test('large integer fields remain exact and unsafe JSON numbers are rejected', async () => {
  const page = await open();
  try {
    await page.getByRole('tab', { name: '更多接口', exact: true }).click();
    await page.getByLabel('搜索接口').fill('SBE 产品深度');
    await page.getByLabel('选择接口').selectOption('books-sbe');
    await page.getByLabel('instIdCode', { exact: true }).fill('9223372036854775807');
    await page.getByRole('button', { name: '查询', exact: true }).click();
    await page.getByText(/"result": "ok"/).waitFor();
    const call = await page.evaluate(() =>
      window.workflowCalls.find((item) => item.command === 'okx_api_request'),
    );
    assert.equal(call.args.request.params.instIdCode, '9223372036854775807');
    await page.getByLabel('搜索接口').fill('批量下单');
    await page.getByLabel('选择接口').selectOption('batch');
    await page
      .getByLabel('批量参数 JSON')
      .fill('[{"instId":"BTC-USDT","sz":"1","id":9223372036854775807}]');
    await page.getByRole('button', { name: '预览操作', exact: true }).click();
    await page
      .getByRole('alert')
      .getByText(/整数.*引号/)
      .waitFor();
    assert.equal(await page.getByRole('dialog', { name: '确认 OKX 操作' }).count(), 0);
  } finally {
    await page.close();
  }
});

test('WebSocket write remains pending until its operation response arrives', async () => {
  const page = await open();
  try {
    await page.getByRole('tab', { name: '实时频道', exact: true }).click();
    await page.getByLabel('选择频道').selectOption('ws-order');
    await page.getByRole('button', { name: '预览操作', exact: true }).click();
    await page.getByRole('button', { name: '确认提交', exact: true }).click();
    await page.getByRole('region', { name: '待核对操作' }).waitFor();
    const requestId = await page.evaluate(
      () =>
        window.workflowCalls.find((item) => item.command === 'okx_ws_start').args.request
          .request_id,
    );
    await page.evaluate(
      (id) =>
        window.streamEventHandler({ kind: 'message', sequence: 1, data: { id, event: 'notice' } }),
      requestId,
    );
    await page.getByText(/"notice"/).waitFor();
    assert.equal(await page.getByRole('region', { name: '待核对操作' }).count(), 1);
    await page.evaluate(
      (id) =>
        window.streamEventHandler({
          kind: 'message',
          sequence: 2,
          data: { id, op: 'order', code: '0', data: [] },
        }),
      requestId,
    );
    await page.getByText(/"op": "order"/).waitFor();
    assert.equal(await page.getByRole('region', { name: '待核对操作' }).count(), 0);
  } finally {
    await page.close();
  }
});

test('conditional strategy fields do not become mandatory across order types', async () => {
  const page = await open();
  try {
    await page.getByRole('tab', { name: '更多接口', exact: true }).click();
    await page.getByLabel('搜索接口').fill('策略委托下单');
    await page.getByLabel('选择接口').selectOption('algo');
    assert.equal(await page.getByLabel('szLimit', { exact: true }).count(), 1);
    await page.getByLabel('ordType', { exact: true }).fill('conditional');
    await page.getByRole('button', { name: '预览操作', exact: true }).click();
    await page.getByRole('dialog', { name: '确认 OKX 操作' }).waitFor();
  } finally {
    await page.close();
  }
});

test('stream started after navigating away is closed immediately', async () => {
  const page = await open();
  try {
    await page.evaluate(() => {
      window.delayStreamStart = true;
    });
    await page.getByRole('tab', { name: '实时频道', exact: true }).click();
    await page.getByLabel('选择频道').selectOption('tickers');
    await page.getByRole('button', { name: '开始订阅', exact: true }).click();
    await page.getByRole('tab', { name: 'OKX 行情', exact: true }).click();
    await page.waitForFunction(() =>
      window.workflowCalls.some(
        (item) => item.command === 'okx_ws_stop' && item.args.sessionId === 'stream-1',
      ),
    );
  } finally {
    await page.close();
  }
});
