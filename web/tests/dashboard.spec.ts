import { test, expect } from "@playwright/test";
import { snapshot, candles } from "./fixture";
test("dashboard shows data, confirms pause, handles outages and fits mobile", async ({
  page,
}) => {
  const state = snapshot();
  let offline = false,
    pausedRequest = false;
  await page.route("**/api/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (offline) return route.abort();
    let value: unknown = {};
    if (path.endsWith("/observability")) {
      state.generated_at = Date.now();
      value = state;
    } else if (path.endsWith("/health"))
      value = {
        status: "ready",
        database: true,
        market_data_fresh: true,
        history_ready: true,
        analysis_ready: true,
        paper_ready: true,
      };
    else if (path.endsWith("/prices"))
      value = {
        fresh: true,
        prices: {
          BTC: "72345.12",
          ETH: "2840.52",
          SOL: "168.21",
          HYPE: "41.08",
        },
      };
    else if (path.endsWith("/candles"))
      value = {
        candles: candles().map((candle) => ({ candle, closed: true })),
      };
    else if (path.endsWith("/evaluations")) value = { evaluations: [] };
    else if (path.endsWith("/paper/control")) {
      pausedRequest = true;
      expect(route.request().headers()["authorization"]).toBe(
        "Bearer test-only-control-key",
      );
      state.account.entries_paused = true;
      state.account.blockers = ["operator_paused"];
      value = { accepted: true };
    }
    await route.fulfill({ json: value });
  });
  const browserErrors: string[] = [];
  page.on("pageerror", (e) => browserErrors.push(e.message));
  page.on("console", (m) => {
    if (m.type() === "error") browserErrors.push(m.text());
  });
  await page.goto("./");
  await expect(page.getByText("Sistema operativo")).toBeVisible();
  await expect(page.getByText("Sin posiciones abiertas")).toBeVisible();
  await expect(page.getByRole("img", { name: "Velas de BTC" })).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Pausar entradas", exact: true }),
  ).toBeDisabled();
  await page.getByRole("button", { name: "Desbloquear controles" }).click();
  await page.getByLabel("Clave de control").fill("test-only-control-key");
  await page.getByRole("button", { name: "Listo", exact: true }).click();
  await page
    .getByRole("button", { name: "Pausar entradas", exact: true })
    .click();
  expect(pausedRequest).toBe(false);
  await page.getByRole("button", { name: "Confirmar", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Iniciar / reanudar" }),
  ).toBeVisible();
  expect(pausedRequest).toBe(true);
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.setViewportSize({ width: 1440, height: 1100 });
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({
    path: "test-results/dashboard-desktop.png",
    fullPage: true,
    animations: "disabled",
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(
    page.getByRole("heading", { name: "Vista general" }),
  ).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    )
    .toBe(true);
  await page.screenshot({
    path: "test-results/dashboard-mobile.png",
    fullPage: true,
    animations: "disabled",
  });
  offline = true;
  await expect(page.getByRole("alert")).toContainText("No se pudo actualizar", {
    timeout: 15000,
  });
  await expect(
    page.getByRole("button", { name: "Iniciar / reanudar" }),
  ).toBeDisabled();
});
test("real service serves bundled UI and persisted observations", async ({
  page,
  request,
}) => {
  test.skip(!process.env.DASHBOARD_URL, "requires running engine");
  const response = await request.get("/api/observability");
  expect(response.ok()).toBe(true);
  const state = await response.json();
  expect(state.mode).toBe("PAPER");
  expect(state.monitor.samples).toBeGreaterThan(0);
  const browserErrors: string[] = [];
  page.on("pageerror", (e) => browserErrors.push(e.message));
  page.on("console", (m) => {
    if (m.type() === "error") browserErrors.push(m.text());
  });
  await page.goto("./");
  await expect(
    page.getByRole("heading", { name: "Vista general" }),
  ).toBeVisible();
  await expect(page.getByRole("img", { name: "Velas de BTC" })).toBeVisible({
    timeout: 20000,
  });
  const canvas = page
    .getByRole("img", { name: "Velas de BTC" })
    .locator("canvas");
  await expect(canvas).toBeVisible();
  await expect
    .poll(() =>
      canvas.evaluate((node: HTMLCanvasElement) => {
        const ctx = node.getContext("2d")!;
        const pixels = ctx.getImageData(0, 0, node.width, node.height).data;
        let visible = 0;
        for (let i = 3; i < pixels.length; i += 4)
          if (pixels[i]! > 0) visible++;
        return visible;
      }),
    )
    .toBeGreaterThan(1000);
  await page.screenshot({
    path: "test-results/dashboard-real.png",
    fullPage: true,
    animations: "disabled",
  });
  await page.getByRole("button", { name: "Mercados", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Última evaluación de estrategias" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Sistema", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Telemetría del proceso" }),
  ).toBeVisible();
  expect(browserErrors).toEqual([]);
  const denied = await request.post("/api/paper/control", {
    data: { command_id: "unauthorized-test", action: "pause" },
  });
  expect(denied.status()).toBe(state.controls_enabled ? 401 : 403);
  const metrics = await request.get("/metrics");
  expect(await metrics.text()).toContain("trading_market_data_fresh");
});
