import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: "tests",
  testMatch: "*.spec.ts",
  use: {
    baseURL: process.env.DASHBOARD_URL || "http://127.0.0.1:5173/dashboard/",
    headless: true,
    screenshot: "only-on-failure",
  },
  webServer: process.env.DASHBOARD_URL
    ? undefined
    : {
        command: "npm run dev",
        url: "http://127.0.0.1:5173/dashboard/",
        reuseExistingServer: !process.env.CI,
      },
  reporter: "list",
});
