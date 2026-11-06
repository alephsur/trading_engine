import { beforeEach, afterEach, describe, it, expect, vi } from "vitest";
import { setActivePinia, createPinia } from "pinia";
import { useDashboard } from "../src/store";
import { snapshot } from "./fixture";
import { number, percent } from "../src/format";
import { equityChart } from "../src/charts";
beforeEach(() => setActivePinia(createPinia()));
afterEach(() => vi.unstubAllGlobals());
describe("data quality and operator commands", () => {
  it("expires a previously valid account without another response", () => {
    const s = useDashboard();
    s.snapshot = snapshot();
    s.received = Date.now();
    s.now = s.received;
    expect(s.fresh).toBe(true);
    s.now += 15001;
    expect(s.fresh).toBe(false);
    expect(number(null)).toBe("—");
    expect(percent(null)).toBe("—");
  });
  it("retries unknown outcomes using the same command without optimistic state", async () => {
    const s = useDashboard();
    s.snapshot = snapshot();
    s.credential = "test";
    const fetch = vi.fn().mockRejectedValue(new TypeError("offline"));
    vi.stubGlobal("fetch", fetch);
    await s.control("pause");
    expect(s.snapshot.account.entries_paused).toBe(false);
    expect(s.pending?.action).toBe("pause");
    await s.control();
    expect(fetch.mock.calls[0]![1].body).toBe(fetch.mock.calls[1]![1].body);
  });
  it("does not allow an old market response to overwrite a new selection", async () => {
    let resolveOld!: (r: Response) => void;
    let count = 0;
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => {
        if (url.includes("BTC") && url.includes("candles"))
          return new Promise<Response>((resolve) => (resolveOld = resolve));
        count++;
        return new Response(
          JSON.stringify(
            url.includes("candles")
              ? {
                  candles: [
                    { candle: { t: 42 }, closed: true },
                    { candle: { t: 43 }, closed: false },
                  ],
                }
              : { evaluations: [] },
          ),
        );
      }),
    );
    const s = useDashboard();
    const old = s.market();
    s.symbol = "ETH";
    await s.market();
    resolveOld(
      new Response(
        JSON.stringify({ candles: [{ candle: { t: 1 }, closed: true }] }),
      ),
    );
    await old;
    expect(s.candles[0]?.t).toBe(42);
    expect(s.candles).toHaveLength(1);
    expect(count).toBe(3);
  });
  it("breaks chart lines across missing observations", () => {
    const option = equityChart([
      { time: 1000, equity: "100" },
      { time: 200000, equity: "110" },
    ]);
    expect((option.series as { data: unknown[] }[])[0]?.data).toEqual([
      [1000, 100],
      [1001, null],
      [200000, 110],
    ]);
  });
});
