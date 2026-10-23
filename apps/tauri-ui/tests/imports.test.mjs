import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
import { installWorkflowIpc } from './helpers/workflow-ipc.mjs';
import { navigate } from './helpers/navigation.mjs';

const { url, newPage } = browserSuite('1449');
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
