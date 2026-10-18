import { test } from 'node:test';
import assert from 'node:assert/strict';
import { browserSuite } from './helpers/browser.mjs';
import { installWorkflowIpc } from './helpers/workflow-ipc.mjs';
const suite = browserSuite(1444);
test('screen capture preserves drafts, requires source consent and OS usage access, surfaces cancel and stops', async () => {
  const page = await suite.newPage();
  await installWorkflowIpc(page);
  await page.addInitScript(() => {
    let sources = [{ package_name: 'com.tencent.mm', enabled: false, account_id: null }];
    window.captureRunning = false;
    window.usageGranted = false;
    window.cancelCapture = true;
    const original = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = async (command, args) => {
      if (command === 'screen_capture_state')
        return { sources: structuredClone(sources), latest: [] };
      if (command === 'screen_capture_status')
        return {
          android: true,
          running: window.captureRunning,
          usage_granted: window.usageGranted,
          code: 'stopped',
        };
      if (command === 'screen_capture_usage_settings') {
        window.usageGranted = true;
        return;
      }
      if (command === 'save_screen_capture_source') {
        sources = [args.source];
        return;
      }
      if (command === 'start_screen_capture') {
        if (window.cancelCapture) throw new Error('屏幕授权已取消');
        window.captureRunning = true;
        return;
      }
      if (command === 'stop_screen_capture') {
        window.captureRunning = false;
        return;
      }
      return original(command, args);
    };
  });
  await page.goto(suite.url);
  await page.locator('#menu').click();
  await page.locator('#nav-notifications').click();
  const card = page.getByRole('article', { name: '支付页面识别' });
  await card.getByLabel('启用识别').check();
  await card.getByLabel('默认记入平台').selectOption('wallet');
  assert.equal(await card.getByRole('button', { name: '授权并开始识别' }).isEnabled(), false);
  await page.waitForTimeout(3200);
  assert.equal(await card.getByLabel('默认记入平台').inputValue(), 'wallet');
  await card.getByLabel('我已了解屏幕捕获和本地识别用途').check();
  await card.getByRole('button', { name: '保存识别来源' }).click();
  await card.getByText('屏幕识别来源设置已保存。').waitFor();
  await card.getByRole('button', { name: '打开使用情况访问设置' }).click();
  await card.getByText('识别服务未运行', { exact: true }).waitFor();
  await card.getByRole('button', { name: '授权并开始识别' }).click();
  await card.getByText('屏幕授权已取消', { exact: true }).waitFor();
  await page.waitForTimeout(3200);
  assert.equal(await card.getByText('屏幕授权已取消', { exact: true }).count(), 1);
  await page.evaluate(() => {
    window.cancelCapture = false;
  });
  await card.getByRole('button', { name: '授权并开始识别' }).click();
  await card.getByRole('button', { name: '停止屏幕识别' }).click();
  await card.getByText('识别服务未运行', { exact: true }).waitFor();
  await page.close();
});
