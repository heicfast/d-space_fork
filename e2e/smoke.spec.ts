import { expect, test, type Page } from '@playwright/test';

/**
 * Golden-path UI smoke (grid #55; ADR-008 flow): home → select device →
 * scan starts DIRECTLY (no configuration step — the reference's
 * scanClickedSlot flips straight to the scan surface) → progress with the
 * corpus status formats → completion → results → filter → preview →
 * select → recover → report. Verifies state transitions and that no
 * console errors or unhandled rejections occur along the way.
 */

let consoleErrors: string[] = [];

function attachErrorCollector(page: Page): void {
  consoleErrors = [];
  page.on('console', (msg) => {
    if (msg.type() === 'error') consoleErrors.push(msg.text());
  });
  page.on('pageerror', (err) => consoleErrors.push(String(err)));
}

test.beforeEach(({ page }) => {
  attachErrorCollector(page);
});

test('golden path: scan → results → recover', async ({ page }) => {
  // 1. Home renders with device cards
  await page.goto('/');
  await expect(page.getByRole('heading', { name: /choose where to look/i })).toBeVisible({
    timeout: 15_000,
  });
  // Scoped to the section heading — the sidebar nav item carries the same
  // label, which would trip Playwright's strict mode via getByText.
  await expect(page.getByRole('heading', { name: /internal drives/i })).toBeVisible();

  // 2. Select the first device, then click the card's scan button: the scan
  //    must start DIRECTLY — no intermediate configuration surface exists.
  await page.getByRole('radio').first().click();
  const scanButton = page.getByRole('button', { name: /search for lost data/i }).first();
  await expect(scanButton).toBeEnabled();
  await scanButton.click();

  // 3. The scan surface appears with the reference status line formats.
  await expect(page.getByText(/quick scanning|advanced scanning/i).first()).toBeVisible({
    timeout: 20_000,
  });
  await page.screenshot({ path: 'test-results/05-scan-live.png' });

  // 4. Wait for completion modal (mock full chain finishes < 60 s)
  const modal = page.getByRole('dialog', { name: /search complete/i });
  await expect(modal).toBeVisible({ timeout: 90_000 });
  await page.screenshot({ path: 'test-results/10-scan-complete.png' });

  // 5. Open results
  await modal.getByRole('button', { name: /go to recover/i }).click();
  await expect(page.getByRole('grid')).toBeVisible({ timeout: 20_000 });
  await expect(page.getByRole('row').nth(1)).toBeVisible();
  await page.screenshot({ path: 'test-results/20-results.png' });

  // 6. Filter bar opens; expand the Type dropdown — its option group renders
  await page.getByRole('button', { name: /^filter$/i }).click();
  await page.getByRole('button', { name: /^type$/i }).click();
  await expect(page.getByRole('group', { name: /^type$/i })).toBeVisible();
  await page.screenshot({ path: 'test-results/30-filter-open.png' });

  // 7. Search — the breadcrumb becomes "Search results for …"
  const search = page.getByLabel(/search by name/i);
  await search.fill('a');
  await search.press('Enter');
  await expect(page.getByText(/search results for/i)).toBeVisible({ timeout: 10_000 });

  // 8. Click the first data row — the complementary Preview pane shows the
  // node's confidence grade; then tick the row checkbox to select it for
  // recovery (checkbox clicks intentionally do not move the preview).
  await page.getByRole('row').nth(1).click();
  const previewPane = page.getByRole('complementary', { name: 'Preview' });
  await expect(previewPane.getByText(/excellent|good|fair|poor|unknown/i)).toBeVisible();
  await page
    .getByLabel(/select row/i)
    .first()
    .click();
  await page.screenshot({ path: 'test-results/40-preview.png' });

  // 9. Recover the selection
  await page.getByRole('button', { name: /recover 1 item/i }).click();
  await expect(page.getByRole('heading', { name: /recover files/i })).toBeVisible();

  // 10. Choose destination and start
  await page.getByRole('button', { name: /choose folder/i }).click();
  await page.getByRole('button', { name: /recover 1 item/i }).click();

  // 11. Report appears — and the export action fires a toast (top-right
  //     stack, MOTION-SPEC §3.3).
  const report = page.getByRole('status').filter({ hasText: /recovery finished/i });
  await expect(report).toBeVisible({ timeout: 60_000 });
  await page.getByRole('button', { name: /export/i }).click();
  await expect(page.getByText(/report exported/i)).toBeVisible({ timeout: 10_000 });
  await page.screenshot({ path: 'test-results/50-report.png' });

  // 12. No console errors along the whole path
  const benign = consoleErrors.filter(
    (e) => e.includes('Download the React DevTools') || e.includes('[vite]'),
  );
  expect(consoleErrors.filter((e) => !benign.includes(e))).toEqual([]);
});

test('quick targets start real scans directly (Recycle Bin)', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: /choose where to look/i })).toBeVisible({
    timeout: 15_000,
  });
  // The "Specify a location" row: Recycle Bin scans immediately (GT-landing §4).
  await page.getByRole('button', { name: /^recycle bin/i }).click();
  await expect(page.getByText(/quick scanning|advanced scanning/i).first()).toBeVisible({
    timeout: 20_000,
  });
});

test('state surfaces: empty scan route falls back to home prompt', async ({ page }) => {
  await page.goto('/#/results/scan-does-not-exist');
  await expect(page.getByText(/try again, or go back/i)).toBeVisible({ timeout: 15_000 });
});

test('window chrome: cluster hidden in browser (host provides controls — no dead buttons)', async ({
  page,
}) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: /choose where to look/i })).toBeVisible({
    timeout: 15_000,
  });
  // The Tauri-only window controls must not render in the browser runtime.
  await expect(page.getByTestId('window-controls')).toHaveCount(0);
  // The title bar itself is present and double-click is inert here.
  const header = page.locator('header[data-tauri-drag-region]');
  await expect(header).toBeVisible();
  await header.dblclick();
  await expect(page.getByRole('heading', { name: /choose where to look/i })).toBeVisible();
});

test('sidebar: collapse/expand with persistence (keyboard-operable toggle)', async ({
  page,
}) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: /choose where to look/i })).toBeVisible({
    timeout: 15_000,
  });
  const nav = page.locator('#sidebar-nav');
  const toggle = page.getByTestId('sidebar-toggle');
  await expect(toggle).toBeVisible();
  await expect(toggle).toHaveAttribute('aria-expanded', 'true');

  // Collapse: rail narrows to 52px and labels fade out of the visual tree.
  await toggle.click();
  await expect(nav).toHaveCSS('width', '52px');
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  const firstLink = nav.getByRole('link').first();
  await expect(firstLink.locator('span')).toHaveCSS('opacity', '0');
  await expect(firstLink.locator('svg')).toBeVisible(); // icons retained
  await page.screenshot({ path: 'test-results/60-sidebar-collapsed.png' });

  // Persistence: reload keeps the collapsed state.
  await page.reload();
  await expect(page.getByTestId('sidebar-toggle')).toHaveAttribute('aria-expanded', 'false');
  await expect(page.locator('#sidebar-nav')).toHaveCSS('width', '52px');

  // Expand back: keyboard path (focus + Enter), full width restored.
  await page.getByTestId('sidebar-toggle').focus();
  await page.keyboard.press('Enter');
  await expect(page.locator('#sidebar-nav')).toHaveCSS('width', '208px');
  await expect(page.getByTestId('sidebar-toggle')).toHaveAttribute('aria-expanded', 'true');
});

test('dialogs: corpus choreography — outside click swallowed, ESC closes, focus returns', async ({
  page,
}) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: /choose where to look/i })).toBeVisible({
    timeout: 15_000,
  });

  // v4 shell: About lives behind the three-lines Menu (aria-label from
  // i18n menu.label). The standalone header About button is gone.
  const menuTrigger = page.getByRole('button', { name: 'Menu' });
  await menuTrigger.click();
  await page.getByRole('menuitem', { name: 'About' }).click();
  await expect(page.getByRole('dialog')).toBeVisible();

  // Outside click on the scrim is swallowed (corpus atlas §12 rule 10).
  await page.mouse.click(10, 300);
  await expect(page.getByRole('dialog')).toBeVisible();

  // ESC closes the dismissible dialog and focus returns to the trigger.
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(menuTrigger).toBeFocused();
});
