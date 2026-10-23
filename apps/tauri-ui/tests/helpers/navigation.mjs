// Follow the visible directory pages, including when switching between navigation roots.
export async function navigate(page, view) {
  const paths = {
    platforms: ['settings', 'sources', 'platforms'],
    notifications: ['settings', 'sources', 'notifications'],
    privacy: ['settings', 'security', 'privacy'],
    data: ['settings', 'security', 'data'],
    sources: ['settings', 'sources'],
    security: ['settings', 'security'],
    reports: ['finance', 'reports'],
    budgets: ['finance', 'budgets'],
    recurring: ['finance', 'recurring'],
    search: ['finance', 'search'],
    reconciliation: ['finance', 'reconciliation'],
  };
  await page.locator('#menu').click();
  for (const destination of paths[view] ?? [view]) {
    await page.locator('#nav-' + destination).click();
  }
}
