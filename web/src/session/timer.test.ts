import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { type Clocks, Deadline, STEP, Timer } from "./timer";

/** Two clocks a test moves on its own; `advance` moves both and runs the timers due. */
function clocks() {
  const now = { wall: 1_000_000, steady: 5_000 };
  const moved: Clocks = { wall: () => now.wall, steady: () => now.steady };
  const advance = async (seconds: number) => {
    now.wall += seconds * 1000;
    now.steady += seconds * 1000;
    await vi.advanceTimersByTimeAsync(seconds * 1000);
  };
  return { now, moved, advance };
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("Deadline", () => {
  it("falls by whichever clock reaches it first", () => {
    const { now, moved } = clocks();
    const due = new Deadline(10, moved);
    expect(due.left()).toBe(10_000);
    now.wall += 4_000;
    expect(due.left()).toBe(6_000);
    now.steady += 7_000;
    expect(due.left()).toBe(3_000);
  });

  it("reads the browser's own clocks when given none", () => {
    expect(new Deadline(10).left()).toBeGreaterThan(9_000);
  });
});

describe("Timer", () => {
  it("runs once its time has passed, and not before", async () => {
    const { moved, advance } = clocks();
    const ran = vi.fn();
    new Timer(moved).after(90, ran);
    await advance(89);
    expect(ran).not.toHaveBeenCalled();
    await advance(1);
    expect(ran).toHaveBeenCalledTimes(1);
  });

  it("sees a sleep, when the wall clock moved on but timers did not run, within a step", async () => {
    const { now, moved, advance } = clocks();
    const ran = vi.fn();
    new Timer(moved).after(600, ran);
    now.wall += 3_600_000;
    await advance(STEP - 1);
    expect(ran).not.toHaveBeenCalled();
    await advance(1);
    expect(ran).toHaveBeenCalledTimes(1);
  });

  it("is not stretched when the wall clock is set back", async () => {
    const { now, moved, advance } = clocks();
    const ran = vi.fn();
    new Timer(moved).after(120, ran);
    now.wall -= 1_800_000;
    await advance(120);
    expect(ran).toHaveBeenCalledTimes(1);
  });
});

describe("Timer, set again", () => {
  it("keeps one timer: setting it again replaces what was set, and stopping it runs nothing", async () => {
    const { moved, advance } = clocks();
    const first = vi.fn();
    const second = vi.fn();
    const timer = new Timer(moved);
    timer.after(10, first);
    timer.after(20, second);
    await advance(20);
    expect([first.mock.calls.length, second.mock.calls.length]).toEqual([0, 1]);
    timer.after(5, first);
    timer.stop();
    await advance(5);
    expect(first).not.toHaveBeenCalled();
  });

  it("runs at once for no time at all", () => {
    const ran = vi.fn();
    new Timer().after(0, ran);
    expect(ran).toHaveBeenCalledTimes(1);
  });
});
