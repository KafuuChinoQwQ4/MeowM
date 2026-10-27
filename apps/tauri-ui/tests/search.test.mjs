import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile } from 'node:fs/promises';
import { browserSuite } from './helpers/browser.mjs';
import { navigate } from './helpers/navigation.mjs';
const { url, newPage } = browserSuite('1435');
async function open(page) {
  await page.addInitScript(() => {
    let categories = [{ id: 'food', name: '餐饮', enabled: true }];
    window.calls = [];
    window.ruleSaved = false;
    window.__TAURI_INTERNALS__ = {
      invoke: async (cmd, args = {}) => {
        if (cmd === 'privacy_status')
          return { locked: false, lock_enabled: false, hide_amounts: false, auto_lock_minutes: 5 };
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
        if (cmd === 'query_entries')
          return {
            items: [
              {
                id: 'item-1',
                status: 'confirmed',
                kind: 'expense',
                occurred_at: '2026-10-01T10:00:00Z',
                amount: '12.34',
                merchant: '星巴克',
                description: '午餐',
                sources: ['manual'],
                external_ids: [],
                edit_version: 0,
                account_id: 'wallet',
                category_id: null,
                original_transaction_id: null,
              },
            ],
            total: 1,
            offset: args.query.offset,
            limit: args.query.limit,
          };
        if (cmd === 'classification_state')
          return {
            categories,
            rules: window.ruleSaved
              ? [
                  {
                    id: 'rule-1',
                    priority: 1,
                    merchant_contains: 'starbucks',
                    description_contains: null,
                    category_id: 'food',
                    enabled: true,
                  },
                ]
              : [],
            aliases: [],
          };
        if (cmd === 'preview_classification_rule')
          return {
            match_count: 3,
            pending_count: 1,
            pending_ids: ['pending-1'],
            pending_samples: [
              { id: 'pending-1', merchant: '示例咖啡', description: '午餐', amount: '12.34' },
            ],
          };
        if (cmd === 'write_classification_rule') {
          window.ruleSaved = true;
          return 'rule-1';
        }
        if (cmd === 'save_merchant_alias') return;
        if (cmd === 'write_category') {
          const id = args.id ?? 'new-category';
          categories = categories.filter((category) => category.id !== id).concat({ ...args, id });
          return id;
        }
        if (cmd === 'delete_category') {
          if (window.rejectCategoryDelete) throw Error('分类已有关联，请先调整关联或停用分类。');
          categories = categories.filter((category) => category.id !== args.id);
          return;
        }
        if (cmd === 'save_search_csv') {
          window.savedCsv = args.csv;
          return;
        }
        if (cmd === 'list_categories') return [{ id: 'food', name: '餐饮' }];
        throw Error(cmd);
      },
    };
  });
  await page.goto(url);
  await navigate(page, 'search');
}
test('search combines filters, clears and exports filtered rows', async () => {
  const page = await newPage();
  try {
    await open(page);
    await page.getByLabel('关键词').fill('星巴克');
    await page.getByLabel('最低金额（元）').fill('10');
    await page.getByLabel('最高金额（元）').fill('20');
    await page.getByRole('button', { name: '搜索流水' }).click();
    await page.getByText('星巴克', { exact: true }).waitFor();
    const q = await page.evaluate(
      () => window.calls.filter((x) => x.cmd === 'query_entries').at(-1).args.query,
    );
    assert.equal(q.keyword, '星巴克');
    assert.equal(q.min_amount, '10');
    assert.equal(q.max_amount, '20');
    const download = page.waitForEvent('download');
    await page.getByRole('button', { name: '导出当前结果' }).click();
    const csv = await download;
    assert.equal(csv.suggestedFilename(), 'meowm-search.csv');
    assert.match(await readFile(await csv.path(), 'utf8'), /星巴克/);
    await page.getByRole('button', { name: '清除筛选' }).click();
    assert.equal(await page.getByLabel('关键词').inputValue(), '');
  } finally {
    await page.close();
  }
});
test('Android export uses the system save command with the filtered CSV', async () => {
  const page = await newPage({ userAgent: 'Mozilla/5.0 (Linux; Android 15) Chrome/124.0' });
  try {
    await open(page);
    await page.getByRole('button', { name: '导出当前结果' }).click();
    await page.waitForFunction(() => !!window.savedCsv);
    const csv = await page.evaluate(() => window.savedCsv);
    assert.match(csv, /星巴克/);
    assert.match(csv, /12\.34/);
  } finally {
    await page.close();
  }
});
test('rule preview precedes save and aliases can be set', async () => {
  const page = await newPage();
  try {
    await open(page);
    await page.getByRole('button', { name: '分类规则' }).click();
    await page.getByRole('button', { name: '新增规则' }).click();
    await page.getByLabel('规则商户包含').fill('starbucks');
    await page.getByRole('button', { name: '预览规则' }).click();
    await page.getByText('命中 3 条，影响 1 条未手工分类的待确认记录').waitFor();
    await page.getByText('示例咖啡 · ¥12.34').waitFor();
    assert.equal(await page.evaluate(() => window.ruleSaved), false);
    await page.getByRole('button', { name: '确认保存规则' }).click();
    assert.equal(await page.evaluate(() => window.ruleSaved), true);
    await page.getByLabel('商户别名').fill('星巴克');
    await page.getByLabel('规范商户').fill('starbucks');
    await page.getByRole('button', { name: '保存别名' }).click();
    assert.ok(
      await page.evaluate(() =>
        window.calls.some((x) => x.cmd === 'save_merchant_alias' && x.args.alias === '星巴克'),
      ),
    );
  } finally {
    await page.close();
  }
});
test('custom merchant classification is saved for future records', async () => {
  const page = await newPage();
  try {
    await open(page);
    await page.getByRole('button', { name: '分类规则' }).click();
    await page.getByLabel('商家名称').fill('我家附近奶茶店');
    await page.getByLabel('归入分类').selectOption('food');
    await page.getByRole('button', { name: '预览商家分类' }).click();
    await page.getByText(/将用于之后的新流水；匹配现有 3 条、其中待确认 1 条/).waitFor();
    await page.getByRole('button', { name: '确认保存商家分类' }).click();
    await page.waitForFunction(() => window.ruleSaved);
    assert.ok(
      await page.evaluate(() =>
        window.calls.some(
          (x) =>
            x.cmd === 'write_classification_rule' &&
            x.args.request.merchant_contains === '我家附近奶茶店' &&
            x.args.request.category_id === 'food',
        ),
      ),
    );
  } finally {
    await page.close();
  }
});
test('category names remain visible on a phone and a new category can be deleted after confirmation', async () => {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  try {
    await open(page);
    await page.getByRole('button', { name: '分类规则', exact: true }).click();
    const name = '日常生活用品采购';
    const input = page.getByLabel('新分类名称', { exact: true });
    await input.fill(name);
    const box = await input.boundingBox();
    assert.ok(box.width >= 230 && box.height >= 40, JSON.stringify(box));
    assert.equal(await input.inputValue(), name);
    await page.getByRole('button', { name: '新增分类', exact: true }).click();
    const category = page.locator('.search-list-row').filter({ hasText: name });
    await category.getByText(`${name} · 启用`, { exact: true }).waitFor();
    assert.equal(await input.inputValue(), '');
    page.once('dialog', (dialog) => dialog.dismiss());
    await category.getByRole('button', { name: `删除分类 ${name}`, exact: true }).click();
    assert.equal(await category.count(), 1);
    page.once('dialog', (dialog) => dialog.accept());
    await category.getByRole('button', { name: `删除分类 ${name}`, exact: true }).click();
    await category.waitFor({ state: 'hidden' });
    assert.equal(await page.getByRole('button', { name: '删除分类 餐饮', exact: true }).count(), 1);
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth > innerWidth),
      false,
    );
  } finally {
    await page.close();
  }
});
test('failed category deletion preserves the category and its enable controls for retry', async () => {
  const page = await newPage();
  try {
    await open(page);
    await page.getByRole('button', { name: '分类规则', exact: true }).click();
    await page.evaluate(() => {
      window.rejectCategoryDelete = true;
    });
    page.once('dialog', (dialog) => dialog.accept());
    await page.getByRole('button', { name: '删除分类 餐饮', exact: true }).click();
    await page
      .getByRole('alert')
      .getByText('分类已有关联，请先调整关联或停用分类。', { exact: true })
      .waitFor();
    const category = page.locator('.search-list-row').filter({ hasText: '餐饮 · 启用' });
    assert.equal(
      await category.getByRole('button', { name: '停用', exact: true }).isEnabled(),
      true,
    );
    await page.evaluate(() => {
      window.rejectCategoryDelete = false;
    });
    page.once('dialog', (dialog) => dialog.accept());
    await category.getByRole('button', { name: '删除分类 餐饮', exact: true }).click();
    await category.waitFor({ state: 'hidden' });
  } finally {
    await page.close();
  }
});
