import assert from 'node:assert/strict';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
import { navigate } from './helpers/navigation.mjs';
const { url, newPage } = browserSuite('1436');
async function open(page, missing = false) {
  await page.addInitScript(
    ({ missing }) => {
      window.calls = [];
      const record = {
        id: 'session',
        account_id: 'wallet',
        date: '2026-10-16',
        cutoff: '2026-10-16T15:59:59.999999Z',
        expected: '-100.00',
        actual: '-101.00',
        difference: '-1.00',
        adjustment_id: null,
        status: 'open',
        version: 0,
        reference: '-100.00',
        reference_at: '2026-10-01T15:59:59Z',
        created_at: '2026-10-16T12:00:00Z',
        period_from: null,
        snapshot_ready: true,
      };
      let started = false,
        reviewed = false;
      window.__TAURI_INTERNALS__ = {
        invoke: async (cmd, args = {}) => {
          if (cmd === 'privacy_status')
            return {
              locked: false,
              lock_enabled: false,
              hide_amounts: false,
              auto_lock_minutes: 5,
            };
          window.calls.push({ cmd, args });
          if (cmd === 'list_accounts') return [{ id: 'wallet', name: '钱包', category: 'wallet' }];
          if (cmd === 'get_day')
            return {
              date: args.date,
              income: '0.00',
              expense: '0.00',
              net: '0.00',
              balance: null,
              reference: null,
              entries: [],
            };
          if (cmd === 'recurring_state')
            return { templates: [], reminders: [], generated_count: 0, has_more: false };
          const summary = () => ({
            record: { ...record },
            entries: [
              {
                id: 'item',
                occurred_at: '2026-10-16T10:00:00Z',
                kind: 'expense',
                status: 'pending',
                delta: '-2.00',
                merchant: '待确认商户',
                description: '尚未入账',
                reviewed,
              },
            ],
            events: [{ action: 'started', occurred_at: record.created_at, reason: null }],
            pending_count: 1,
          });
          if (cmd === 'reconciliation_list') return started ? [{ ...record }] : [];
          if (cmd === 'reconciliation_start') {
            if (missing) throw Error('请先为该平台设置参考余额，再开始对账');
            started = true;
            return summary();
          }
          if (cmd === 'reconciliation_summary') return summary();
          if (cmd === 'reconciliation_review') {
            assertVersion(args);
            reviewed = args.reviewed;
            record.version++;
            record.status = reviewed ? 'reviewed' : 'open';
            return;
          }
          if (cmd === 'reconciliation_adjust') {
            assertVersion(args);
            record.adjustment_id = 'adjustment';
            record.version++;
            return 'adjustment';
          }
          if (cmd === 'reconciliation_close') {
            assertVersion(args);
            record.status = 'closed';
            record.version++;
            return;
          }
          if (cmd === 'reconciliation_reopen') {
            assertVersion(args);
            record.status = 'reviewed';
            record.version++;
            return;
          }
          throw Error(cmd);
          function assertVersion(a) {
            if (a.version !== record.version) throw Error('会话已变化');
          }
        },
      };
    },
    { missing },
  );
  await page.goto(url);
  await navigate(page, 'reconciliation');
  await page.getByLabel('截止日期').fill('2026-10-16');
  await page.getByLabel('实际余额（元）').fill('-101.00');
  await page.getByRole('button', { name: '开始对账' }).click();
}
test('back from a reconciliation detail stays within reconciliation', async () => {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  try {
    await open(page);
    await page.getByRole('heading', { name: '对账快照' }).waitFor();
    const back = page.getByRole('button', { name: '返回上一级', exact: true });
    assert.equal(await back.isEnabled(), true);
    await back.click();
    assert.equal(await page.locator('#nav-finance').getAttribute('class'), 'nav-item active');
    assert.equal(await page.getByRole('heading', { name: '对账快照' }).count(), 0);
    assert.equal(await back.isEnabled(), true);
    await page.locator('.reconciliation-history-item').click();
    await page.getByRole('heading', { name: '对账快照' }).waitFor();
    assert.equal(await back.isEnabled(), true);
    await page.locator('#menu').click();
    await page.locator('#nav-ledger').click();
    assert.equal(await back.isDisabled(), true);
    await navigate(page, 'reconciliation');
    assert.equal(await back.isEnabled(), true);
    await back.click();
    await page.getByRole('heading', { name: '财务管理', exact: true }).waitFor();
    assert.equal(await back.isDisabled(), true);
    assert.equal(await page.getByRole('heading', { name: '对账快照' }).count(), 0);
  } finally {
    await page.close();
  }
});

test('reviews snapshot then explicitly adjusts and closes/reopens with current versions', async () => {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  try {
    await open(page);
    await page.getByRole('heading', { name: '对账快照' }).waitFor();
    await page.getByText('待确认商户', { exact: true }).waitFor();
    assert.equal(await page.getByRole('button', { name: '确认生成余额调整' }).isEnabled(), false);
    assert.equal(await page.getByRole('button', { name: '关闭对账' }).isEnabled(), false);
    await page.getByLabel('已核对：待确认商户').check();
    await page.getByLabel('调整原因').fill('核对后补差');
    page.once('dialog', (d) => d.accept());
    await page.getByRole('button', { name: '确认生成余额调整' }).click();
    await page.getByText('已生成余额调整', { exact: true }).waitFor();
    await page.getByRole('button', { name: '关闭对账' }).click();
    await page.getByRole('button', { name: '重新打开对账' }).click();
    await page.getByRole('button', { name: '关闭对账' }).waitFor();
    const changes = await page.evaluate(() =>
      window.calls.filter((c) =>
        ['reconciliation_adjust', 'reconciliation_close', 'reconciliation_reopen'].includes(c.cmd),
      ),
    );
    assert.deepEqual(
      changes.map((c) => c.args.version),
      [1, 2, 3],
    );
    assert.equal(changes[0].args.reason, '核对后补差');
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth > innerWidth),
      false,
    );
  } finally {
    await page.close();
  }
});
test('missing reference is actionable and failed start preserves entered balance', async () => {
  const page = await newPage();
  try {
    await open(page, true);
    await page.getByRole('alert').filter({ hasText: '请先为该平台设置参考余额' }).waitFor();
    assert.equal(await page.getByLabel('实际余额（元）').inputValue(), '-101.00');
    assert.equal(await page.getByRole('heading', { name: '对账快照' }).count(), 0);
  } finally {
    await page.close();
  }
});
