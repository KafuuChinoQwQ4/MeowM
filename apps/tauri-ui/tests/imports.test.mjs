import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
import { installWorkflowIpc } from './helpers/workflow-ipc.mjs';
import { navigate } from './helpers/navigation.mjs';

const { url, newPage } = browserSuite('1449');

test('encrypted PDF prompts for a password and retries the original file without retaining the password', async () => {
  const page = await newPage();
  try {
    await installWorkflowIpc(page);
    await page.addInitScript(() => {
      const original = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (command === 'preview_bill' && !args.previewId) {
          if (args.password !== 'fixture-only')
            throw args.password ? 'PDF 密码不正确，请重新输入' : 'PDF 已加密，请输入打开密码';
        }
        return original(command, args);
      };
    });
    await page.goto(url);
    await navigate(page, 'pending');
    await page.getByRole('button', { name: '导入账单', exact: true }).click();
    const dialog = page.getByRole('dialog');
    await dialog.getByLabel('来源', { exact: true }).selectOption('bank');
    const buffer = readFileSync(
      new URL('../../../crates/ledger-import/tests/fixtures/boc-encrypted.pdf', import.meta.url),
    );
    await dialog.locator('input[type=file]').setInputFiles({
      name: 'bank.pdf',
      mimeType: 'application/pdf',
      buffer,
    });
    const password = dialog.getByLabel('PDF 打开密码', { exact: true });
    await password.waitFor();
    assert.equal(await password.getAttribute('type'), 'password');
    await password.fill('wrong');
    await dialog.getByRole('button', { name: '解锁并读取账单', exact: true }).click();
    await dialog
      .getByRole('alert')
      .getByText(/密码不正确/)
      .waitFor();
    assert.equal(await password.inputValue(), '');
    await password.fill('fixture-only');
    await dialog.getByRole('button', { name: '解锁并读取账单', exact: true }).click();
    await dialog.getByRole('button', { name: '加入待确认收件箱' }).waitFor();
    assert.equal(await dialog.locator('input[type=password]').count(), 0);
    const requests = await page.evaluate(() =>
      window.workflowCalls.filter((call) => call.command === 'preview_bill'),
    );
    assert.equal(requests.at(-1).args.password, 'fixture-only');
    assert.deepEqual(Buffer.from(requests.at(-1).args.bytes), buffer);
    await dialog.getByLabel('记入平台', { exact: true }).selectOption('bank');
    await page.waitForFunction(() =>
      window.workflowCalls.some((call) => call.command === 'preview_bill' && call.args.previewId),
    );
    const last = await page.evaluate(
      () => window.workflowCalls.filter((call) => call.command === 'preview_bill').at(-1).args,
    );
    assert.equal(last.accountId, 'bank');
    assert.equal(last.password ?? null, null);
    assert.equal(last.bytes.length, 0);
  } finally {
    await page.close();
  }
});

for (const [direction, source, destination] of [
  ['outgoing', 'boc', 'alipay'],
  ['incoming', 'alipay', 'boc'],
]) {
  test(`bank ${direction} transfer uses the signed amount and counterparty to prefill both platforms`, async () => {
    const page = await newPage();
    try {
      await installWorkflowIpc(page);
      await page.addInitScript((transferDirection) => {
        const original = window.__TAURI_INTERNALS__.invoke;
        window.__TAURI_INTERNALS__.invoke = async (command, args) => {
          if (command === 'list_accounts')
            return [
              { id: 'boc', name: '中国银行', category: 'bank' },
              { id: 'alipay', name: '支付宝', category: 'wallet' },
            ];
          const result = await original(command, args);
          if (command === 'preview_bill') {
            result.rows = [
              {
                ...result.rows[0],
                id: 'transfer',
                kind: 'transfer',
                transfer_direction: transferDirection,
                merchant: '支付宝（中国）网络技术有限公司',
                description: '网上转账',
              },
            ];
            result.new_count = result.pending_count = 1;
            result.exact_duplicates = result.suspected_duplicates = 0;
          }
          return result;
        };
      }, direction);
      await page.goto(url);
      await navigate(page, 'pending');
      await page.getByRole('button', { name: '导入账单', exact: true }).click();
      const sourceSelect = page.locator('dialog').getByLabel('来源', { exact: true });
      assert.ok((await sourceSelect.locator('option').allTextContents()).includes('中国银行'));
      await sourceSelect.selectOption('boc');
      await page.locator('dialog input[type=file]').setInputFiles({
        name: 'bank.csv',
        mimeType: 'text/csv',
        buffer: Buffer.from('synthetic'),
      });
      await page.getByRole('button', { name: '加入待确认收件箱' }).waitFor();
      assert.equal(await page.getByLabel('记入平台', { exact: true }).inputValue(), 'boc');
      const previewRequest = await page.evaluate(
        () => window.workflowCalls.find((call) => call.command === 'preview_bill').args,
      );
      assert.equal(previewRequest.provider, 'bank');
      assert.equal(previewRequest.accountId, 'boc');
      assert.equal(await page.getByLabel('第 1 行转出平台').inputValue(), source);
      assert.equal(await page.getByLabel('第 1 行转入平台').inputValue(), destination);
      await page.getByRole('button', { name: '加入待确认收件箱' }).click();
      await page.waitForFunction(() =>
        window.workflowCalls.some((call) => call.command === 'stage_bill'),
      );
      const args = await page.evaluate(
        () => window.workflowCalls.find((call) => call.command === 'stage_bill').args,
      );
      assert.equal(args.edits[0].account_id, source);
      assert.equal(args.edits[0].destination_id, destination);
    } finally {
      await page.close();
    }
  });
}

for (const destination of ['wallet', 'bank']) {
  test(`uncounted transfer accepts ${destination === 'wallet' ? 'internal' : 'cross-platform'} flow and preserves its order IDs`, async () => {
    const page = await newPage();
    try {
      await installWorkflowIpc(page);
      await page.addInitScript(() => {
        const original = window.__TAURI_INTERNALS__.invoke;
        window.__TAURI_INTERNALS__.invoke = async (command, args) => {
          if (command !== 'preview_bill') return original(command, args);
          const preview = await original(command, args);
          preview.rows = [
            {
              ...preview.rows[0],
              id: 'transfer',
              kind: 'transfer',
              merchant: '',
              description: '余额提现',
              transaction_number: '00001234',
              merchant_order_id: '00005678',
              account_hint: '银行|余额',
            },
          ];
          preview.new_count = 1;
          preview.exact_duplicates = 0;
          preview.suspected_duplicates = 0;
          preview.pending_count = 1;
          return preview;
        };
      });
      await page.goto(url);
      await navigate(page, 'pending');
      await page.getByRole('button', { name: '导入账单', exact: true }).click();
      await page.locator('dialog input[type=file]').setInputFiles({
        name: 'transfer.csv',
        mimeType: 'text/csv',
        buffer: Buffer.from('synthetic'),
      });
      await page.getByText('渠道交易单号：00001234', { exact: true }).waitFor();
      await page.getByText('商户单号：00005678', { exact: true }).waitFor();
      await page.getByRole('button', { name: '加入待确认收件箱' }).click();
      await page
        .getByRole('alert')
        .getByText(/转账记录需选择/)
        .waitFor();
      assert.equal(
        await page.evaluate(() =>
          window.workflowCalls.some((call) => call.command === 'stage_bill'),
        ),
        false,
      );
      await page.getByLabel('第 1 行转出平台').selectOption('wallet');
      await page.getByLabel('第 1 行转入平台').selectOption(destination);
      await page.getByRole('button', { name: '加入待确认收件箱' }).click();
      await page.waitForFunction(() =>
        window.workflowCalls.some((call) => call.command === 'stage_bill'),
      );
      const args = await page.evaluate(
        () => window.workflowCalls.find((call) => call.command === 'stage_bill').args,
      );
      assert.deepEqual(args.ids, ['transfer']);
      assert.equal(args.edits[0].kind, 'transfer');
      assert.equal(args.edits[0].account_id, 'wallet');
      assert.equal(args.edits[0].destination_id, destination);
    } finally {
      await page.close();
    }
  });
}

test('submit jumps to each transfer row with a missing platform mapping', async () => {
  const page = await newPage();
  try {
    await page.setViewportSize({ width: 390, height: 460 });
    await installWorkflowIpc(page);
    await page.addInitScript(() => {
      const original = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        const result = await original(command, args);
        if (command === 'preview_bill' && !args.previewId) {
          const template = result.rows[0];
          result.rows = [
            { ...template, id: 'expense', row: 1, kind: 'expense', merchant: '普通消费' },
            {
              ...template,
              id: 'outgoing',
              row: 2,
              kind: 'transfer',
              transfer_direction: 'outgoing',
              merchant: '未匹配平台甲',
              description: '转账',
            },
            {
              ...template,
              id: 'incoming',
              row: 3,
              kind: 'transfer',
              transfer_direction: 'incoming',
              merchant: '未匹配平台乙',
              description: '转账',
            },
          ];
          result.new_count = result.pending_count = 3;
          result.exact_duplicates = result.suspected_duplicates = 0;
        }
        return result;
      };
    });
    await page.goto(url);
    await navigate(page, 'pending');
    await page.getByRole('button', { name: '导入账单', exact: true }).click();
    const dialog = page.getByRole('dialog');
    await dialog.getByLabel('来源', { exact: true }).selectOption('bank');
    await dialog.locator('input[type=file]').setInputFiles({
      name: 'transfers.csv',
      mimeType: 'text/csv',
      buffer: Buffer.from('synthetic'),
    });
    const submit = dialog.getByRole('button', { name: '加入待确认收件箱' });
    await submit.waitFor();

    await submit.click();
    await page.waitForFunction(
      () => document.activeElement?.getAttribute('aria-label') === '第 2 行转入平台',
      null,
      { timeout: 1500 },
    );
    const firstTarget = await page.evaluate(() => ({
      scrollTop: document.querySelector('.import-preview-form fieldset').scrollTop,
      rowClass: document.querySelector('[data-import-row-id="outgoing"]')?.className,
    }));
    assert.ok(firstTarget.scrollTop > 0);
    assert.match(firstTarget.rowClass, /import-row-needs-attention/);

    await page.getByLabel('第 2 行转入平台').selectOption('wallet');
    await submit.click();
    await page.waitForFunction(
      () => document.activeElement?.getAttribute('aria-label') === '第 3 行转出平台',
      null,
      { timeout: 1500 },
    );
    assert.match(
      await page.locator('[data-import-row-id="incoming"]').getAttribute('class'),
      /import-row-needs-attention/,
    );

    await page.getByLabel('第 3 行转出平台').selectOption('wallet');
    await submit.click();
    await page.waitForFunction(() =>
      window.workflowCalls.some((call) => call.command === 'stage_bill'),
    );
    const args = await page.evaluate(
      () => window.workflowCalls.find((call) => call.command === 'stage_bill').args,
    );
    assert.deepEqual(args.ids, ['expense', 'outgoing', 'incoming']);
    assert.equal(args.edits.find((edit) => edit.id === 'outgoing').destination_id, 'wallet');
    assert.equal(args.edits.find((edit) => edit.id === 'incoming').account_id, 'wallet');
  } finally {
    await page.close();
  }
});

test('Alipay import selects its platform and can mark a Huabei repayment as an internal flow', async () => {
  const page = await newPage();
  try {
    await installWorkflowIpc(page);
    await page.addInitScript(() => {
      const original = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (command === 'list_accounts')
          return [
            { id: 'wallet', name: '微信', category: 'wallet' },
            { id: 'alipay', name: '支付宝', category: 'wallet' },
          ];
        const result = await original(command, args);
        if (command === 'preview_bill') {
          result.rows = [
            {
              ...result.rows[0],
              kind: 'transfer',
              description: '花呗主动还款',
              merchant: '',
              account_hint: '银行卡|花呗',
            },
          ];
          result.new_count = result.pending_count = 1;
          result.exact_duplicates = result.suspected_duplicates = 0;
        }
        return result;
      };
    });
    await page.goto(url);
    await navigate(page, 'pending');
    await page.getByRole('button', { name: '导入账单', exact: true }).click();
    await page.locator('dialog').getByLabel('来源', { exact: true }).selectOption('alipay');
    await page.locator('dialog input[type=file]').setInputFiles({
      name: 'alipay.csv',
      mimeType: 'text/csv',
      buffer: Buffer.from('synthetic'),
    });
    assert.equal(await page.getByLabel('记入平台', { exact: true }).inputValue(), 'alipay');
    const previewArgs = await page.evaluate(
      () => window.workflowCalls.find((call) => call.command === 'preview_bill').args,
    );
    assert.equal(previewArgs.accountId, 'alipay');
    await page.getByRole('button', { name: '记为当前平台内部划转' }).click();
    assert.equal(await page.getByLabel('第 1 行转出平台').inputValue(), 'alipay');
    assert.equal(await page.getByLabel('第 1 行转入平台').inputValue(), 'alipay');
    await page.getByText(/平台内部划转：保留记录/).waitFor();
    await page.getByRole('button', { name: '加入待确认收件箱' }).click();
    await page.waitForFunction(() =>
      window.workflowCalls.some((call) => call.command === 'stage_bill'),
    );
    const args = await page.evaluate(
      () => window.workflowCalls.find((call) => call.command === 'stage_bill').args,
    );
    assert.equal(args.accountId, 'alipay');
    assert.equal(args.edits[0].account_id, 'alipay');
    assert.equal(args.edits[0].destination_id, 'alipay');
  } finally {
    await page.close();
  }
});

test('import source lists every managed platform and routes each managed bank through the bank parser', async () => {
  const page = await newPage();
  try {
    await installWorkflowIpc(page);
    await page.addInitScript(() => {
      const original = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (command === 'list_accounts')
          return [
            { id: 'boc', name: '中国银行', category: 'bank' },
            { id: 'cmb', name: '招商银行', category: 'bank' },
            { id: 'abc', name: '农业银行', category: 'bank' },
            { id: 'wechat', name: '微信', category: 'wallet' },
            { id: 'alipay', name: '支付宝', category: 'wallet' },
            { id: 'paypal', name: 'PayPal', category: 'wallet' },
            { id: 'cash', name: '现金', category: 'cash' },
          ];
        return original(command, args);
      };
    });
    await page.goto(url);
    await navigate(page, 'pending');
    await page.getByRole('button', { name: '导入账单', exact: true }).click();

    const dialog = page.getByRole('dialog');
    const source = dialog.getByLabel('来源', { exact: true });
    const optionLabels = await source.locator('option').allTextContents();
    for (const name of ['中国银行', '招商银行', '农业银行', '微信', '支付宝', 'PayPal', '现金']) {
      assert.ok(
        optionLabels.some((label) => label.includes(name)),
        `${name} should be visible`,
      );
    }
    assert.equal(
      await source.locator('option[value="paypal"]').evaluate((option) => option.disabled),
      true,
    );
    assert.equal(
      await source.locator('option[value="cash"]').evaluate((option) => option.disabled),
      true,
    );

    await source.selectOption('cmb');
    await dialog.locator('input[type=file]').setInputFiles({
      name: 'cmb.csv',
      mimeType: 'text/csv',
      buffer: Buffer.from('日期,金额\n2026-10-03,-12.30'),
    });
    await dialog.getByRole('button', { name: '加入待确认收件箱' }).waitFor();
    const args = await page.evaluate(
      () => window.workflowCalls.find((call) => call.command === 'preview_bill').args,
    );
    assert.equal(args.provider, 'bank');
    assert.equal(args.accountId, 'cmb');
    assert.equal(await dialog.getByLabel('记入平台', { exact: true }).inputValue(), 'cmb');
  } finally {
    await page.close();
  }
});

for (const [name, mimeType, provider, buffer] of [
  ['bank.csv', 'text/csv', 'bank', Buffer.from('日期,金额\n2026-10-03,-12.30')],
  [
    'alipay.xls',
    'application/vnd.ms-excel',
    'alipay',
    readFileSync(
      new URL('../../../crates/ledger-import/tests/fixtures/alipay.xls', import.meta.url),
    ),
  ],
  [
    'wechat.xlsx',
    'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
    'wechat',
    readFileSync(
      new URL('../../../crates/ledger-import/tests/fixtures/wechat.xlsx', import.meta.url),
    ),
  ],
]) {
  test(`bill picker accepts ${name} and sends original bytes to preview`, async () => {
    const page = await newPage();
    try {
      await installWorkflowIpc(page);
      await page.goto(url);
      await page.locator('.balance-value').waitFor();
      await navigate(page, 'pending');
      await page.getByRole('button', { name: '导入账单', exact: true }).click();
      await page.locator('dialog').getByLabel('来源', { exact: true }).selectOption(provider);
      const input = page.locator('dialog input[type=file]');
      const extensions = (await input.getAttribute('accept')).split(',');
      assert.ok(extensions.includes('.' + name.split('.').at(-1)));
      await input.setInputFiles({ name, mimeType, buffer });
      await page.getByRole('button', { name: '加入待确认收件箱' }).waitFor();
      const args = await page.evaluate(
        () => window.workflowCalls.find((call) => call.command === 'preview_bill').args,
      );
      assert.equal(args.provider, provider);
      assert.deepEqual(Buffer.from(args.bytes), buffer);
    } finally {
      await page.close();
    }
  });
}

test('import confirmation stays at the bottom while a long preview list scrolls', async () => {
  const page = await newPage();
  try {
    await page.setViewportSize({ width: 390, height: 460 });
    await installWorkflowIpc(page);
    await page.goto(url);
    await page.locator('.balance-value').waitFor();
    await navigate(page, 'pending');
    await page.getByRole('button', { name: '导入账单', exact: true }).click();
    await page.locator('dialog input[type=file]').setInputFiles({
      name: 'long.csv',
      mimeType: 'text/csv',
      buffer: Buffer.from('日期,金额\n2026-10-03,12.30'),
    });
    const button = page.getByRole('button', { name: '加入待确认收件箱' });
    await button.waitFor();

    const layout = await page.evaluate(() => {
      const dialog = document.querySelector('dialog');
      const fieldset = dialog.querySelector('.import-preview-form fieldset');
      const button = dialog.querySelector('.import-preview-actions button');
      for (let i = 0; i < 20; i++) {
        const row = document.createElement('div');
        row.style.height = '80px';
        row.style.flex = 'none';
        fieldset.append(row);
      }
      const before = button.getBoundingClientRect();
      fieldset.scrollTop = fieldset.scrollHeight;
      const after = button.getBoundingClientRect();
      return {
        listCanScroll: fieldset.scrollHeight > fieldset.clientHeight,
        scrollTop: fieldset.scrollTop,
        buttonTopBefore: before.top,
        buttonTopAfter: after.top,
        buttonBottom: after.bottom,
        dialogBottom: dialog.getBoundingClientRect().bottom,
        viewportBottom: window.innerHeight,
      };
    });

    assert.ok(layout.listCanScroll);
    assert.ok(layout.scrollTop > 0);
    assert.ok(Math.abs(layout.buttonTopAfter - layout.buttonTopBefore) < 1);
    assert.ok(layout.buttonBottom <= layout.dialogBottom);
    assert.ok(layout.buttonBottom <= layout.viewportBottom);
  } finally {
    await page.close();
  }
});
