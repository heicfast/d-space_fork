import { expect, test, type Page } from '@playwright/test';

/**
 * Responsiveness + DPI verification (mandate §4 parameters 1–3):
 * - window-size range: 800×600 (the Tauri min) → 1920×1080 — no horizontal
 *   scroll, no clipped controls, nothing unreachable
 * - aspect resilience: square-ish (1024×768) and wide (1920×1080)
 * - DPI: 100% (dsf 1) and 150% (dsf 1.5) render the same layout invariants
 *
 * Screenshots at each configuration are archived by the CI workflow
 * (smoke-screens artifact) — 100% and a non-100% DPI are both captured.
 */

async function assertLayoutInvariants(page: Page, label: string): Promise<void> {
  // No horizontal document scroll at any supported size.
  const scrollWidth = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(scrollWidth, `${label}: no horizontal overflow`).toBeLessThanOrEqual(0);

  // The primary CTA is reachable (in view, not clipped).
  const cta = page.getByRole('button', { name: /search for lost data/i }).first();
  await expect(cta, `${label}: CTA visible`).toBeVisible();

  // The sidebar toggle works at this size (collapse does not break layout).
  const nav = page.locator('#sidebar-nav');
  await page.getByTestId('sidebar-toggle').click();
  await expect(nav).toHaveCSS('width', '52px');
  await expect(cta, `${label}: CTA visible after collapse`).toBeVisible();
  await page.getByTestId('sidebar-toggle').click();
  await expect(nav).toHaveCSS('width', '208px');
}

test.describe('responsive + DPI', () => {
  for (const [label, viewport] of [
    ['min 800x600', { width: 800, height: 600 }],
    ['square 1024x768', { width: 1024, height: 768 }],
    ['wide 1920x1080', { width: 1920, height: 1080 }],
  ] as const) {
    test(`home layout invariants at ${label}`, async ({ page }) => {
      await page.setViewportSize(viewport);
      await page.goto('/');
      await expect(page.getByRole('heading', { name: /choose where to look/i })).toBeVisible({
        timeout: 15_000,
      });
      await assertLayoutInvariants(page, label);
      await page.screenshot({
        path: `test-results/70-responsive-${label.replace(/\s+/g, '-')}.png`,
      });
    });
  }

  test('DPI 150% renders the same layout invariants (dsf 1.5)', async ({ browser }) => {
    const context = await browser.newContext({
      viewport: { width: 1240, height: 840 },
      deviceScaleFactor: 1.5,
    });
    const page = await context.newPage();
    await page.goto('/');
    await expect(page.getByRole('heading', { name: /choose where to look/i })).toBeVisible({
      timeout: 15_000,
    });
    await assertLayoutInvariants(page, 'dsf1.5');
    await page.screenshot({ path: 'test-results/71-dpi-150.png' });
    await context.close();
  });

  test('results 3-pane stays usable at the 800×600 minimum', async ({ page }) => {
    await page.setViewportSize({ width: 800, height: 600 });
    await page.goto('/');
    await expect(page.getByRole('heading', { name: /choose where to look/i })).toBeVisible({
      timeout: 15_000,
    });
    // Collapse the sidebar to reclaim width, then enter the quick-target
    // scan (fast) and wait for the results grid.
    await page.getByTestId('sidebar-toggle').click();
    await page.getByRole('button', { name: /^recycle bin/i }).click();
    await expect(page.getByRole('grid')).toBeVisible({ timeout: 90_000 });
    await page.screenshot({ path: 'test-results/72-results-800x600.png' });

    // The Recover CTA stays reachable at the minimum size.
    await expect(page.getByRole('button', { name: /recover all/i }).first()).toBeVisible();
    const scrollWidth = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(scrollWidth, 'results: no horizontal overflow at min size').toBeLessThanOrEqual(0);
  });

  test('keyboard flow: focus lands on main after navigation; primary task reachable by keyboard', async ({
    page,
  }) => {
    await page.goto('/');
    await expect(page.getByRole('heading', { name: /choose where to look/i })).toBeVisible({
      timeout: 15_000,
    });

    // Navigate to Disk Health via keyboard (sidebar link), then verify
    // focus landed on the main landmark — the SPA focus contract.
    const health = page.getByRole('link', { name: /disk health/i });
    await health.focus();
    await page.keyboard.press('Enter');
    await expect(
      page.getByRole('heading', { name: /disk health|s\.m\.a\.r\.t/i }),
    ).toBeVisible();
    const activeId = await page.evaluate(() => document.activeElement?.id);
    expect(activeId, 'focus lands on the main landmark after route change').toBe(
      'main-content',
    );

    // Keyboard-operable primary task entry: back home via the sidebar
    // link, then activate the Recycle Bin quick target by keyboard.
    await page.getByRole('link', { name: /internal drives/i }).focus();
    await page.keyboard.press('Enter');
    await expect(page.getByRole('heading', { name: /choose where to look/i })).toBeVisible();
    await page.getByRole('button', { name: /^recycle bin/i }).focus();
    await page.keyboard.press('Enter');
    await expect(page.getByText(/quick scanning|advanced scanning/i).first()).toBeVisible({
      timeout: 20_000,
    });
  });
});
