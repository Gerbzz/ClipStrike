import { test, expect } from '@playwright/test';

test('web app loads and renders canvas shell', async ({ page }) => {
  const pageErrors: string[] = [];
  const consoleErrors: string[] = [];

  page.on('pageerror', (err) => pageErrors.push(err.stack || err.message || String(err)));
  page.on('console', (msg) => {
    if (msg.type() === 'error') {
      consoleErrors.push(msg.text());
    }
  });

  const response = await page.goto('/');
  expect(response?.ok()).toBeTruthy();

  const canvas = page.locator('#the_canvas_id');
  await expect(canvas).toBeVisible();

  await page.waitForTimeout(3000);

  expect(consoleErrors, `console errors: ${consoleErrors.join(' | ')}`).toEqual([]);
  expect(pageErrors, `page errors: ${pageErrors.join(' | ')}`).toEqual([]);
});
