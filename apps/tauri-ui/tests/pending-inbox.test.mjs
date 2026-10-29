import assert from 'node:assert/strict';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
const { url, newPage } = browserSuite(process.env.PENDING_INBOX_TEST_PORT || '1427');

async function openInbox(page, categoryFailsOnce = false, internalTransfer = false) {
  page.setDefaultTimeout(5000);
  await page.addInitScript(
    ({ categoryFailsOnce, internalTransfer }) => {
      let confirmed = false;
      let categoryAttempts = 0;
      const account = '00000000-0000-4000-8000-000000000001';
      window.ledgerCalls = [];
      // The browser has no native Tauri runtime. Only the IPC boundary is mocked;
      // tests exercise real navigation, React forms and cache refresh.
      window.__TAURI_INTERNALS__ = {
        invoke: async (command, args) => {
          if (command === 'privacy_status')
            return {
              locked: false,
              lock_enabled: false,
              hide_amounts: false,
              auto_lock_minutes: 5,
            };
          window.ledgerCalls.push(command);
          if (command === 'recurring_state')
            return { templates: [], reminders: [], generated_count: 0, has_more: false };
          if (command === 'list_accounts')
            return [{ id: account, name: '测试钱包', category: 'wallet' }];
          if (command === 'get_day')
            return {
              date: args.date,
              income: '0.00',
              expense: confirmed && !internalTransfer ? '12.30' : '0.00',
              net: confirmed && !internalTransfer ? '-12.30' : '0.00',
              balance: confirmed && !internalTransfer ? '87.70' : '100.00',
              reference: { date: args.date, amount: '100.00' },
              entries: confirmed
                ? [
                    {
                      id: 'confirmed',
                      kind: internalTransfer ? 'transfer' : 'expense',
                      amount: '-12.30',
                      note: '测试商户',
                      time: '12:00',
                    },
                  ]
                : [],
            };
          if (command === 'query_entries')
            return {
              items: confirmed
                ? []
                : [
                    {
                      id: '00000000-0000-4000-8000-000000000002',
                      status: 'pending',
                      kind: internalTransfer ? 'transfer' : 'expense',
                      occurred_at: new Date().toISOString(),
                      amount: '12.30',
                      merchant: '测试商户',
                      description: '测试',
                      edit_version: 0,
                      reversal_of: null,
                      account_id: account,
                      destination_id: internalTransfer ? account : null,
                      category_id: null,
                      original_transaction_id: null,
                      sources: ['wechat'],
                      external_ids: [],
                    },
                  ],
              total: confirmed ? 0 : 1,
              offset: 0,
              limit: 20,
            };
          if (command === 'confirm_pending') {
            window.confirmPendingArgs = args;
            confirmed = true;
            return;
          }
          if (command === 'list_categories') {
            if (categoryFailsOnce && categoryAttempts++ === 0) throw new Error('分类读取失败');
            return [{ id: 'food', name: '餐饮' }];
          }
          throw new Error(`Unexpected IPC command: ${command}`);
        },
      };
    },
    { categoryFailsOnce, internalTransfer },
  );
  await page.goto(url);
  await page.waitForFunction(
    () => document.querySelector('.balance-value')?.textContent === '¥ 100.00',
  );
  await page.locator('#menu').click();
  await page.locator('#nav-pending').click();
  await page.getByText('测试商户', { exact: true }).waitFor();
}

test('pending internal transfer keeps its platform selectable when editing and confirming', async () => {
  const page = await newPage();
  try {
    await openInbox(page, false, true);
    await page.getByRole('button', { name: '编辑 / 确认', exact: true }).click();
    const account = '00000000-0000-4000-8000-000000000001';
    await page.getByLabel('转入平台', { exact: true }).selectOption(account);
    await page.getByRole('button', { name: '保存并确认', exact: true }).click();
    await page.waitForFunction(() => window.confirmPendingArgs);
    const args = await page.evaluate(() => window.confirmPendingArgs);
    assert.equal(args.edits[0].kind, 'transfer');
    assert.equal(args.edits[0].account_id, account);
    assert.equal(args.edits[0].destination_id, account);
  } finally {
    await page.close();
  }
});

test('confirm all uses the current filters across pages, prevents repeat clicks and resets pagination', async () => {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  try {
    await openInbox(page);
    await page.evaluate(() => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      let complete = false;
      window.confirmAllCalls = [];
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (command === 'query_entries') {
          const original = await invoke(command, args);
          const items = Array.from({ length: 61 }, (_, i) => ({
            ...original.items[0],
            id: `pending-${i}`,
            merchant: `跨页商户 ${i + 1}`,
          }));
          return {
            items: complete
              ? []
              : items.slice(args.query.offset, args.query.offset + args.query.limit),
            total: complete ? 0 : 61,
            offset: args.query.offset,
            limit: args.query.limit,
          };
        }
        if (command === 'confirm_all_pending') {
          window.confirmAllCalls.push(args);
          await new Promise((resolve) => (window.finishConfirmAll = resolve));
          complete = true;
          return 61;
        }
        return invoke(command, args);
      };
    });
    await page.getByLabel('来源', { exact: true }).selectOption('wechat');
    await page
      .getByLabel('平台', { exact: true })
      .selectOption('00000000-0000-4000-8000-000000000001');
    await page.getByLabel('起始日期', { exact: true }).fill('2026-10-01');
    await page.getByLabel('结束日期（不含）', { exact: true }).fill('2026-11-01');
    await page.getByText('跨页商户 1', { exact: true }).waitFor();
    await page.getByRole('button', { name: '下一页', exact: true }).click();
    await page.getByText('跨页商户 21', { exact: true }).waitFor();
    assert.equal(await page.getByLabel('选择本页全部记录').isChecked(), false);
    const confirmAll = page.getByRole('button', { name: '一键确认全部（61 条）', exact: true });
    await confirmAll.evaluate((button) => {
      button.click();
      button.click();
    });
    await page.waitForFunction(() => window.confirmAllCalls.length > 0);
    assert.equal(
      await page.getByRole('button', { name: '正在确认全部…', exact: true }).isDisabled(),
      true,
    );
    assert.equal(
      await page.getByRole('button', { name: '下一页', exact: true }).isDisabled(),
      true,
    );
    assert.equal(await page.getByLabel('来源', { exact: true }).isDisabled(), true);
    const calls = await page.evaluate(() => window.confirmAllCalls);
    assert.equal(calls.length, 1);
    assert.deepEqual(calls[0].query, {
      status: 'pending',
      source: 'wechat',
      account_id: '00000000-0000-4000-8000-000000000001',
      from_date: '2026-10-01',
      until_date: '2026-11-01',
      offset: 20,
      limit: 20,
    });
    await page.evaluate(() => window.finishConfirmAll());
    await page.getByRole('status').getByText('已确认 61 条记录。', { exact: true }).waitFor();
    await page.getByText('没有符合条件的记录', { exact: true }).waitFor();
    assert.equal(await page.locator('.record-pager span').innerText(), '1 / 1');
    assert.equal(
      await page.getByRole('button', { name: '一键确认全部（0 条）', exact: true }).isDisabled(),
      true,
    );
    await page.getByLabel('状态', { exact: true }).selectOption('ignored');
    assert.equal(await page.getByRole('button', { name: /一键确认全部/ }).count(), 0);
  } finally {
    await page.close();
  }
});

test('failed confirm all keeps records available and permits retry', async () => {
  const page = await newPage();
  try {
    await openInbox(page);
    await page.evaluate(() => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (command === 'confirm_all_pending') throw Error('关联的原消费尚未确认');
        return invoke(command, args);
      };
    });
    await page.getByLabel('选择本页全部记录').check();
    const button = page.getByRole('button', { name: '一键确认全部（1 条）', exact: true });
    await button.click();
    await page.getByRole('alert').getByText('关联的原消费尚未确认', { exact: true }).waitFor();
    assert.equal(await button.isEnabled(), true);
    assert.equal(await page.getByLabel('选择本页全部记录').isChecked(), true);
    assert.equal(await page.getByText('测试商户', { exact: true }).count(), 1);
  } finally {
    await page.close();
  }
});

for (const mode of ['single', 'bulk']) {
  test(`${mode} pending confirmation refreshes totals and balance when returning to ledger`, async () => {
    const page = await newPage({ viewport: { width: 430, height: 820 } });
    const errors = [];
    page.on('pageerror', (error) => errors.push(error.message));
    try {
      await openInbox(page);
      if (mode === 'bulk') {
        await page.getByLabel('选择本页全部记录').check();
        await page.getByRole('button', { name: '批量确认', exact: true }).click();
      } else {
        await page.getByRole('button', { name: '编辑 / 确认', exact: true }).click();
        await page.getByRole('button', { name: '保存并确认', exact: true }).click();
        await page.locator('#sheet').waitFor({ state: 'hidden' });
      }
      await page.getByText('没有符合条件的记录').waitFor();
      await page.locator('#menu').click();
      await page.locator('#nav-ledger').click();
      await page.waitForFunction(() => document.querySelector('.balance-value') !== null);
      assert.equal(await page.locator('.balance-value').textContent(), '¥ 87.70');
      assert.match(await page.locator('.summary-card.expense').textContent(), /12\.30/);
      assert.ok(
        (await page.evaluate(
          () => window.ledgerCalls.filter((command) => command === 'get_day').length,
        )) > 1,
      );
      assert.deepEqual(errors, []);
    } finally {
      await page.close();
    }
  });
}

test('category loading failure is visible and can be retried', async () => {
  const page = await newPage();
  try {
    await openInbox(page, true);
    await page.getByRole('button', { name: '编辑 / 确认', exact: true }).click();
    await page.getByText('分类读取失败', { exact: false }).waitFor();
    await page.getByRole('button', { name: '重试分类', exact: true }).click();
    await page.waitForFunction(() =>
      [...document.querySelectorAll('option')].some((option) => option.textContent === '餐饮'),
    );
  } finally {
    await page.close();
  }
});

test('duplicate review excludes distinct orders and weak same-amount matches', async () => {
  const page = await newPage();
  try {
    await openInbox(page);
    await page.evaluate(() => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (command !== 'query_entries') return invoke(command, args);
        const base = {
          id: 'current-order',
          status: 'pending',
          kind: 'expense',
          occurred_at: '2026-10-02T12:00:00Z',
          amount: '12.30',
          merchant: '测试商户',
          description: '测试',
          edit_version: 0,
          reversal_of: null,
          account_id: '00000000-0000-4000-8000-000000000001',
          category_id: null,
          original_transaction_id: null,
          sources: ['wechat'],
          external_ids: [{ source: 'wechat', id: 'order-1' }],
        };
        const items = args.query.status
          ? [base]
          : [
              base,
              {
                ...base,
                id: 'distinct-order',
                external_ids: [{ source: 'wechat', id: 'order-2' }],
              },
              { ...base, id: 'hours-apart', occurred_at: '2026-10-02T15:00:00Z', external_ids: [] },
              { ...base, id: 'different-merchant', merchant: '其他商户', external_ids: [] },
              { ...base, id: 'different-kind', kind: 'income', external_ids: [] },
              { ...base, id: 'unknown-merchant', merchant: null, external_ids: [] },
              { ...base, id: 'unknown-amount', amount: null, external_ids: [] },
              {
                ...base,
                id: 'nearby-match',
                occurred_at: '2026-10-02T12:02:00Z',
                external_ids: [],
              },
              { ...base, id: 'same-platform-id', occurred_at: '2026-10-02T15:00:00Z' },
              {
                ...base,
                id: 'cross-source-match',
                external_ids: [{ source: 'bank', id: 'bank-1' }],
              },
            ];
        return { items, total: items.length, offset: 0, limit: 200 };
      };
    });
    await page.getByRole('button', { name: '刷新待确认记录', exact: true }).click();
    const details = page.locator('.duplicate-details');
    await details.locator('summary').click();
    await details.getByText(/nearby-match/).waitFor();
    const text = await details.innerText();
    for (const id of ['nearby-match', 'same-platform-id', 'cross-source-match']) {
      assert.ok(text.includes(id), `missing duplicate candidate ${id}`);
    }
    for (const id of [
      'distinct-order',
      'hours-apart',
      'different-merchant',
      'different-kind',
      'unknown-merchant',
      'unknown-amount',
    ]) {
      assert.ok(!text.includes(id), `unexpected duplicate candidate ${id}`);
    }
  } finally {
    await page.close();
  }
});

async function openRefundInbox(page, refundMatches = []) {
  await openInbox(page);
  await page.evaluate((refundMatches) => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    const base = {
      id: '00000000-0000-4000-8000-000000000002',
      status: 'pending',
      kind: 'expense',
      occurred_at: '2026-10-02T12:00:00Z',
      amount: '12.30',
      merchant: '测试商户',
      description: '测试',
      edit_version: 0,
      reversal_of: null,
      reversal_id: null,
      account_id: '00000000-0000-4000-8000-000000000001',
      category_id: null,
      original_transaction_id: null,
      sources: ['wechat'],
      external_ids: [],
    };
    let pending = [
      base,
      {
        ...base,
        id: '00000000-0000-4000-8000-000000000003',
        kind: 'refund',
        amount: '3.00',
        merchant: '退款商户',
        refund_matches: refundMatches,
      },
    ];
    const confirmed = Array.from({ length: 22 }, (_, index) => ({
      ...base,
      id: `00000000-0000-4000-8000-${String(index + 10).padStart(12, '0')}`,
      status: 'confirmed',
      kind: 'income',
      merchant: '收入记录',
    }));
    confirmed[1] = { ...confirmed[1], kind: 'expense', reversal_of: base.id, merchant: '冲销记录' };
    confirmed[2] = {
      ...confirmed[2],
      kind: 'expense',
      merchant: '原消费商户',
      description: '午餐订单',
    };
    confirmed[3] = {
      ...confirmed[3],
      kind: 'expense',
      merchant: '已撤销原消费',
      reversal_id: 'reversal-on-another-page',
    };
    confirmed[20] = { ...confirmed[20], kind: 'expense', merchant: '更早原消费' };
    window.refundMutations = [];
    window.__TAURI_INTERNALS__.invoke = async (command, args) => {
      if (command === 'query_entries') {
        const query = args.query;
        const items =
          query.status === 'confirmed'
            ? confirmed.filter(
                (item) =>
                  !query.keyword || `${item.merchant} ${item.description}`.includes(query.keyword),
              )
            : pending;
        return {
          items: items.slice(query.offset, query.offset + query.limit),
          total: items.length,
          offset: query.offset,
          limit: query.limit,
        };
      }
      if (command === 'confirm_pending' || command === 'edit_pending') {
        window.refundMutations.push({ command, args });
        if (command === 'confirm_pending') {
          pending = pending.filter((item) => !args.ids.includes(item.id));
        } else {
          pending = pending.map((item) =>
            item.id === args.edit.id
              ? { ...item, ...args.edit, edit_version: item.edit_version + 1 }
              : item,
          );
        }
        return;
      }
      return invoke(command, args);
    };
  }, refundMatches);
  await page.getByRole('button', { name: '刷新待确认记录', exact: true }).click();
  await page.getByText('退款商户', { exact: true }).waitFor();
}

test('unlinked refunds can be bulk confirmed together with expenses', async () => {
  const page = await newPage();
  try {
    await openRefundInbox(page);
    await page.getByLabel('选择本页全部记录').check();
    await page.getByRole('button', { name: '批量确认', exact: true }).click();
    await page.getByText('没有符合条件的记录', { exact: true }).waitFor();
    assert.deepEqual(await page.evaluate(() => window.refundMutations), [
      {
        command: 'confirm_pending',
        args: {
          ids: ['00000000-0000-4000-8000-000000000002', '00000000-0000-4000-8000-000000000003'],
          edits: [],
        },
      },
    ]);
  } finally {
    await page.close();
  }
});

test('refund editor supports optional manual linking with searching and paging', async () => {
  const page = await newPage();
  try {
    await openRefundInbox(page);
    await page
      .locator('.pending-card')
      .filter({ hasText: '退款商户' })
      .getByRole('button', { name: '编辑 / 确认', exact: true })
      .click();
    const original = page.getByLabel('退款原消费', { exact: true });
    await original
      .locator('option')
      .filter({ hasText: '原消费商户' })
      .waitFor({ state: 'attached' });
    assert.equal(await original.inputValue(), '');
    assert.ok(!(await original.textContent()).includes('收入记录'));
    assert.ok(!(await original.textContent()).includes('冲销记录'));
    assert.ok(!(await original.textContent()).includes('已撤销原消费'));
    await page.getByRole('button', { name: '下一页原消费', exact: true }).click();
    await page.waitForFunction(() =>
      [...document.querySelectorAll('option')].some((item) =>
        item.textContent.includes('更早原消费'),
      ),
    );
    await page.getByLabel('搜索原消费（商户、备注或订单号）', { exact: true }).fill('午餐订单');
    await page.waitForFunction(() =>
      [...document.querySelectorAll('option')].some((item) =>
        item.textContent.includes('原消费商户'),
      ),
    );
    assert.ok(!(await original.textContent()).includes('更早原消费'));
    await original.selectOption('00000000-0000-4000-8000-000000000012');
    await page.getByRole('button', { name: '保存并确认', exact: true }).click();
    await page.locator('#sheet').waitFor({ state: 'hidden' });
    const mutations = await page.evaluate(() => window.refundMutations);
    assert.equal(mutations.length, 1);
    assert.equal(mutations[0].command, 'confirm_pending');
    assert.deepEqual(mutations[0].args.ids, ['00000000-0000-4000-8000-000000000003']);
    assert.equal(
      mutations[0].args.edits[0].original_transaction_id,
      '00000000-0000-4000-8000-000000000012',
    );
    assert.ok(await page.getByText('测试商户', { exact: true }).isVisible());
  } finally {
    await page.close();
  }
});

test('refund edits can be saved without an original while awaiting review', async () => {
  const page = await newPage();
  try {
    await openRefundInbox(page);
    await page
      .locator('.pending-card')
      .filter({ hasText: '退款商户' })
      .getByRole('button', { name: '编辑 / 确认', exact: true })
      .click();
    await page.getByLabel('备注', { exact: true }).fill('等待核对原消费');
    await page.getByRole('button', { name: '仅保存修改', exact: true }).click();
    await page.locator('#sheet').waitFor({ state: 'hidden' });
    const mutations = await page.evaluate(() => window.refundMutations);
    assert.equal(mutations.length, 1);
    assert.equal(mutations[0].command, 'edit_pending');
    assert.equal(mutations[0].args.edit.original_transaction_id, null);
    assert.equal(mutations[0].args.edit.description, '等待核对原消费');
    assert.ok(await page.getByText('退款商户', { exact: true }).isVisible());
  } finally {
    await page.close();
  }
});

for (const mode of [
  'unlinked',
  'suggested',
  'exact',
  'exact-opt-out',
  'exact-save-only',
  'exact-edit-amount',
  'exact-edit-date',
]) {
  test(`refund editor confirms with ${mode} association`, async () => {
    const page = await newPage();
    try {
      await openRefundInbox(
        page,
        mode === 'unlinked'
          ? []
          : [
              {
                id: '00000000-0000-4000-8000-000000000012',
                confidence: mode === 'suggested' ? 'suggested' : 'exact',
                reason: '测试匹配原因',
                merchant: '原消费商户',
                amount: '12.30',
                occurred_at: '2026-10-01T12:00:00Z',
                account_id: '00000000-0000-4000-8000-000000000001',
                status: 'confirmed',
              },
            ],
      );
      await page
        .locator('.pending-card')
        .filter({ hasText: '退款商户' })
        .getByRole('button', { name: '编辑 / 确认', exact: true })
        .click();
      const original = page.getByLabel('退款原消费', { exact: true });
      await original.waitFor();
      if (mode === 'suggested') {
        assert.equal(await original.inputValue(), '');
        await page.getByRole('button', { name: '关联这笔消费', exact: true }).click();
        assert.equal(await original.inputValue(), '00000000-0000-4000-8000-000000000012');
      } else if (mode.startsWith('exact')) {
        assert.equal(await original.inputValue(), '00000000-0000-4000-8000-000000000012');
        if (mode === 'exact-opt-out') await original.selectOption('');
        if (mode === 'exact-edit-amount') {
          await page.getByLabel('金额（元）', { exact: true }).fill('20.00');
          assert.equal(await original.inputValue(), '');
        }
        if (mode === 'exact-edit-date') {
          await page.getByLabel('日期', { exact: true }).fill('2026-09-01');
          assert.equal(await original.inputValue(), '');
        }
      }
      await page
        .getByRole('button', {
          name: mode === 'exact-save-only' ? '仅保存修改' : '保存并确认',
          exact: true,
        })
        .click();
      await page.locator('#sheet').waitFor({ state: 'hidden' });
      const mutations = await page.evaluate(() => window.refundMutations);
      assert.equal(mutations.length, 1);
      assert.equal(
        (mode === 'exact-save-only' ? mutations[0].args.edit : mutations[0].args.edits[0])
          .original_transaction_id,
        ['suggested', 'exact'].includes(mode) ? '00000000-0000-4000-8000-000000000012' : null,
      );
    } finally {
      await page.close();
    }
  });
}
