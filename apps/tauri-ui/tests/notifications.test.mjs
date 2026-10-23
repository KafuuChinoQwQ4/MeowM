import { test } from 'node:test';
import assert from 'node:assert/strict';
import { browserSuite } from './helpers/browser.mjs';
import { navigate } from './helpers/navigation.mjs';
import { installWorkflowIpc } from './helpers/workflow-ipc.mjs';
const suite = browserSuite(1433);
test('notification settings auto-save rounded control changes after consent', async () => {
  const page = await suite.newPage();
  await installWorkflowIpc(page);
  await page.addInitScript(() => {
    const original = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = async (command, args) => {
      if (command === 'notification_state') {
        if (window.holdSettings)
          await new Promise((resolve) => {
            window.releaseSettings = resolve;
          });
        return {
          sources: JSON.parse(
            localStorage.getItem('sources') ||
              '[{"package_name":"com.tencent.mm","enabled":false,"account_id":null},{"package_name":"com.eg.android.AlipayGphone","enabled":false,"account_id":null}]',
          ),
          latest: [
            {
              package_name: 'com.tencent.mm',
              code: 'needs_confirmation',
              received_at_ms: 1760000000000,
              transaction_id: 'notification-1',
            },
          ],
        };
      }
      if (command === 'notification_access')
        return { android: true, granted: !!window.permissionGranted, connected: false };
      if (command === 'open_notification_settings') {
        window.permissionGranted = true;
        return;
      }
      if (command === 'save_notification_source') {
        if (window.failSourceSave) throw new Error('保存失败，请重试');
        const all = JSON.parse(
          localStorage.getItem('sources') ||
            '[{"package_name":"com.tencent.mm","enabled":false,"account_id":null},{"package_name":"com.eg.android.AlipayGphone","enabled":false,"account_id":null}]',
        );
        all[all.findIndex((s) => s.package_name === args.source.package_name)] = args.source;
        localStorage.setItem('sources', JSON.stringify(all));
        return;
      }
      return original(command, args);
    };
  });
  await page.goto(suite.url);
  await navigate(page, 'notifications');
  await page.getByText('未授予通知访问权限', { exact: true }).waitFor();
  const card = page.getByRole('group', { name: '微信通知来源' });
  const controlStyle = await card.evaluate((root) => {
    const checkbox = root.querySelector('input[type="checkbox"]');
    const select = root.querySelector('select');
    return {
      checkboxRadius: getComputedStyle(checkbox).borderRadius,
      checkboxTransition: getComputedStyle(checkbox).transitionProperty,
      selectRadius: getComputedStyle(select).borderRadius,
    };
  });
  assert.equal(controlStyle.checkboxRadius, '8px');
  assert.match(controlStyle.checkboxTransition, /background-color/);
  assert.equal(controlStyle.selectRadius, '12px');
  assert.equal(await card.getByRole('button', { name: '保存来源' }).count(), 0);
  await page.getByLabel('我已了解并同意通知采集用途').check();
  await card.getByLabel('启用采集').check();
  await card.getByText('来源已启用', { exact: true }).waitFor();
  await card.getByLabel('默认记入平台').selectOption('wallet');
  await page.waitForFunction(() => localStorage.getItem('sources')?.includes('wallet'));
  await page.evaluate(() => {
    window.failSourceSave = true;
  });
  await card.getByLabel('启用采集').uncheck();
  await card.getByRole('alert').waitFor();
  await page.evaluate(() => {
    window.holdSettings = true;
  });
  await page.getByRole('button', { name: '刷新采集状态' }).click();
  await page.waitForFunction(() => typeof window.releaseSettings === 'function');
  assert.equal(await card.count(), 1, 'refresh must preserve the editing form');
  await page.evaluate(() => {
    window.holdSettings = false;
    window.releaseSettings();
  });
  await card.getByText('来源已启用', { exact: true }).waitFor();
  assert.equal(await card.getByLabel('启用采集').isChecked(), false);
  assert.equal(await card.getByLabel('默认记入平台').inputValue(), 'wallet');
  await page.evaluate(() => {
    window.failSourceSave = false;
  });
  await card.getByLabel('启用采集').check();
  await card.getByText('来源已启用', { exact: true }).waitFor();
  await page.getByRole('button', { name: '打开系统通知访问设置' }).click();
  await page.getByText('已授权，监听服务尚未连接', { exact: true }).waitFor();
  await page.reload();
  await navigate(page, 'notifications');
  await card.getByText('来源已启用', { exact: true }).waitFor();
  assert.equal(await card.getByLabel('启用采集').isChecked(), true);
  assert.equal(await card.getByLabel('默认记入平台').inputValue(), 'wallet');
  await page.close();
});
