import { test, expect } from "@playwright/test";
import { snapshot } from "./fixture";
test("ChatGPT connects, selects a model and keeps analysis separate from execution", async ({
  page,
  context,
}) => {
  let connected = false,
    selected: string | null = null,
    analysis: unknown = null,
    fail = false;
  const calls: string[] = [];
  await context.route("https://auth.openai.com/**", (r) =>
    r.fulfill({ body: "OAuth fixture — no real login" }),
  );
  await page.route("**/api/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    let value: unknown = {};
    if (path.includes("/ai/")) {
      calls.push(path);
      if (path.endsWith("/status"))
        value = {
          available: true,
          connected,
          active: connected ? "oaiapp_test" : null,
          profiles: [{ id: "oaiapp_test", email: "fixture@example.test" }],
          selected_model: selected,
          last_analysis: analysis,
        };
      else {
        expect(route.request().headers()["x-trading-client"]).toBe("dashboard");
        if (path.endsWith("/connect")) {
          connected = true;
          value = {
            url: "https://auth.openai.com/api/accounts/authorize?state=fixture",
          };
        }
        if (path.endsWith("/models"))
          value = {
            models: [{ id: "fixture-model", name: "Modelo de prueba" }],
          };
        if (path.endsWith("/model"))
          selected = route.request().postDataJSON().model;
        if (path.endsWith("/analyze")) {
          if (fail)
            return route.fulfill({
              status: 502,
              json: {
                error: "Respuesta incompleta; no se guardó el análisis.",
              },
            });
          analysis = {
            text: "Análisis consultivo de prueba.",
            model: selected,
            created_at_ms: Date.now(),
            context: { source: "public_market_feed_not_testnet" },
          };
          value = analysis;
        }
        if (path.endsWith("/disconnect")) {
          connected = false;
          selected = null;
          value = { remote_revocation_confirmed: false };
        }
      }
    } else if (path.endsWith("/observability")) value = snapshot();
    else if (path.endsWith("/health")) value = { status: "ready" };
    else if (path.endsWith("/candles")) value = { candles: [] };
    else if (path.endsWith("/evaluations")) value = { evaluations: [] };
    else if (path.endsWith("/prices")) value = { prices: {}, fresh: false };
    await route.fulfill({ json: value });
  });
  await page.goto("./");
  await page.getByRole("button", { name: "Sistema", exact: true }).click();
  const panel = page.getByRole("region", { name: "Asistente ChatGPT" });
  await expect(
    panel.getByText("Sin cuenta conectada", { exact: true }),
  ).toBeVisible();
  const popupPromise = page.waitForEvent("popup");
  await panel.getByRole("button", { name: "Continuar con ChatGPT" }).click();
  const popup = await popupPromise;
  await expect(popup).toHaveURL(/https:\/\/auth.openai.com\//);
  await popup.close();
  await expect(panel.getByText(/Cuenta conectada/)).toBeVisible({
    timeout: 15000,
  });
  await panel
    .getByLabel("Modelo disponible en tu cuenta")
    .selectOption("fixture-model");
  await panel.getByRole("button", { name: "Guardar modelo" }).click();
  await expect(
    panel.getByRole("button", { name: "Probar análisis" }),
  ).toBeEnabled();
  await panel.getByRole("button", { name: "Probar análisis" }).click();
  await expect(
    panel.getByText("Análisis consultivo de prueba.", { exact: true }),
  ).toBeVisible();
  fail = true;
  await panel.getByRole("button", { name: "Probar análisis" }).click();
  await expect(
    panel.getByText("Respuesta incompleta; no se guardó el análisis.", {
      exact: true,
    }),
  ).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await expect
    .poll(() =>
      page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    )
    .toBe(true);
  expect(await page.evaluate(() => localStorage.length)).toBe(0);
  await panel.getByRole("button", { name: "Desconectar ChatGPT" }).click();
  await expect(
    panel.getByText(/No se confirmó la revocación remota/),
  ).toBeVisible();
  expect(calls.some((p) => p.includes("control"))).toBe(false);
});
