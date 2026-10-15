import { after, before } from 'node:test';
import { spawn } from 'node:child_process';
import { chromium } from 'playwright';

export function browserSuite(port) {
  const url = `http://127.0.0.1:${port}`;
  let browser;
  let server;
  before(async () => {
    server = spawn(
      process.execPath,
      [
        'node_modules/vite/bin/vite.js',
        '--host',
        '127.0.0.1',
        '--port',
        String(port),
        '--strictPort',
      ],
      { cwd: new URL('../..', import.meta.url), stdio: 'ignore' },
    );
    for (let attempt = 0; attempt < 100; attempt++) {
      if (server.exitCode !== null) throw new Error('Vite test server exited');
      try {
        if ((await fetch(url)).ok) break;
      } catch {}
      if (attempt === 99) throw new Error('Vite test server did not start');
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    browser = await chromium.launch({
      headless: true,
      executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH || undefined,
      args: ['--no-sandbox'],
    });
  });
  after(async () => {
    await browser?.close();
    server?.kill();
  });
  return {
    url,
    newPage: async (options = {}) => {
      const page = await browser.newPage(options);
      page.setDefaultTimeout(5000);
      return page;
    },
  };
}
