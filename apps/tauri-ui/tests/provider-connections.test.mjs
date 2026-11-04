import assert from 'node:assert/strict';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
import { installWorkflowIpc } from './helpers/workflow-ipc.mjs';
import { navigate } from './helpers/navigation.mjs';

const { url, newPage } = browserSuite(process.env.PROVIDER_CONNECTION_TEST_PORT || '1436');

async function open() {
  const page = await newPage({ viewport: { width: 430, height: 820 } });
  await installWorkflowIpc(page);
  await page.goto(url);
  await page.locator('.balance-value').waitFor();
  return page;
}

test('payment APIs live under settings and safely save, test, clear and disconnect', async () => {
  const page = await open();
  try {
    await navigate(page, 'connections');
    await page.getByRole('heading', { name: '支付 API', exact: true }).waitFor();
    await page.getByRole('heading', { name: '微信支付', exact: true }).waitFor();
    await page.getByRole('heading', { name: '支付宝', exact: true }).waitFor();
    const card = page.locator('[data-provider="wechat_pay"]');
    await card.getByLabel('商户号', { exact: true }).fill('merchant-123');
    await card.getByLabel('证书序列号', { exact: true }).fill('serial-123');
    const privateKey = card.getByLabel('商户私钥 PEM', { exact: true });
    await privateKey.fill('PRIVATE-SECRET');
    assert.equal(
      await privateKey.evaluate((element) =>
        getComputedStyle(element).getPropertyValue('-webkit-text-security'),
      ),
      'disc',
    );
    await card.getByRole('button', { name: '保存凭据', exact: true }).click();
    await card.getByText('已安全保存，凭据不会在此页面回显。', { exact: true }).waitFor();
    assert.equal(await card.getByLabel('商户私钥 PEM', { exact: true }).inputValue(), '');
    await card.getByLabel('账单日期', { exact: true }).fill('2026-10-30');
    await card.getByRole('button', { name: '测试账单读取', exact: true }).click();
    await card.getByText(/2026-10-30/).waitFor();
    assert.equal(
      await page.evaluate(() =>
        window.workflowCalls.some((call) => call.command === 'write_transaction'),
      ),
      false,
    );
    await card.getByRole('button', { name: '断开连接', exact: true }).click();
    await card.getByText('尚未配置', { exact: true }).waitFor();
  } finally {
    await page.close();
  }
});

test('exchange directory opens only the selected empty setup form and returns to choose another provider', async () => {
  const page = await open();
  try {
    await navigate(page, 'markets');
    await page.getByRole('heading', { name: '交易所', exact: true }).waitFor();
    for (const title of ['币安', '欧易', '同花顺 iFinD'])
      await page.getByRole('button', { name: `打开${title}`, exact: true }).waitFor();
    assert.equal(await page.locator('#day-content input').count(), 0);
    assert.equal(await page.getByRole('tablist', { name: 'OKX 功能' }).count(), 0);
    await page.getByText('更多交易所，后续接入', { exact: true }).waitFor();
    await page.getByRole('button', { name: '打开欧易', exact: true }).click();
    const okx = page.locator('[data-provider="okx"]');
    await okx.getByLabel('API Key', { exact: true }).waitFor();
    for (const name of ['API Key', 'Secret Key', 'Passphrase'])
      assert.equal(await okx.getByLabel(name, { exact: true }).inputValue(), '');
    await okx.getByLabel('API Key', { exact: true }).fill('unsaved-key');
    assert.equal(await page.locator('[data-provider="binance"]').count(), 0);
    assert.equal(await page.getByRole('tablist', { name: 'OKX 功能' }).count(), 0);
    assert.equal(
      await page.evaluate(() => window.workflowCalls.some((c) => c.command.startsWith('okx_'))),
      false,
    );
    await page.getByRole('button', { name: '返回上一级', exact: true }).click();
    await page.getByRole('button', { name: '打开币安', exact: true }).click();
    await page.locator('[data-provider="binance"]').waitFor();
    assert.equal(await page.locator('[data-provider="okx"]').count(), 0);
    await page.getByRole('button', { name: '返回上一级', exact: true }).click();
    await page.getByRole('button', { name: '打开欧易', exact: true }).click();
    assert.equal(await okx.getByLabel('API Key', { exact: true }).inputValue(), '');
    await navigate(page, 'exchange');
    await page.getByRole('heading', { name: '法币汇率', exact: true }).waitFor();
  } finally {
    await page.close();
  }
});

test('exchange API credentials and account results are masked while iFinD is labeled market data', async () => {
  const page = await open();
  try {
    await navigate(page, 'markets');
    await page.getByRole('button', { name: '打开币安', exact: true }).click();
    const card = page.locator('[data-provider="binance"]');
    await card.getByLabel('API Key', { exact: true }).fill('api-key-secret');
    await card.getByLabel('Secret Key', { exact: true }).fill('secret-key-secret');
    await card.getByRole('button', { name: '保存凭据', exact: true }).click();
    await page.getByRole('button', { name: 'API 设置', exact: true }).waitFor();
    assert.equal(await card.getByLabel('API Key', { exact: true }).count(), 0);
    await card.getByRole('button', { name: '测试只读余额', exact: true }).click();
    await card.getByText('USDT', { exact: true }).waitFor();
    assert.equal(await card.getByText('api-key-secret', { exact: true }).count(), 0);
    assert.equal(
      await page.evaluate(() =>
        window.workflowCalls.some((call) => /trade|withdraw|transfer|order/i.test(call.command)),
      ),
      false,
    );
    await page.getByRole('button', { name: 'API 设置', exact: true }).click();
    assert.equal(await card.getByLabel('API Key', { exact: true }).inputValue(), '');
    await page.getByRole('button', { name: '返回上一级', exact: true }).click();
    await card.getByRole('button', { name: '测试只读余额', exact: true }).waitFor();
    assert.equal(await card.getByLabel('API Key', { exact: true }).count(), 0);
    await navigate(page, 'exchange');
    await navigate(page, 'markets');
    await page.getByRole('button', { name: '打开币安', exact: true }).click();
    await card.getByRole('button', { name: '测试只读余额', exact: true }).waitFor();
    assert.equal(await card.getByLabel('API Key', { exact: true }).count(), 0);
    await page.getByRole('button', { name: 'API 设置', exact: true }).click();
    await card.getByRole('button', { name: '断开连接', exact: true }).click();
    await card.getByLabel('API Key', { exact: true }).waitFor();
    assert.equal(await card.getByLabel('API Key', { exact: true }).inputValue(), '');
    assert.equal(
      await card.getByRole('button', { name: '测试只读余额', exact: true }).isEnabled(),
      false,
    );
    await page.getByRole('button', { name: '返回上一级', exact: true }).click();
    await page.getByRole('button', { name: '打开同花顺 iFinD', exact: true }).click();
    await page.getByText('行情数据服务，不连接证券账户或下单功能。', { exact: true }).waitFor();
    assert.equal(await page.getByText('USDT', { exact: true }).count(), 0);
  } finally {
    await page.close();
  }
});
