// Time for the session island: one pending timer, and a deadline, each measured on two clocks. The wall clock moves
// on while a machine sleeps, when timers may not run; the steady clock never steps back when the wall clock is
// corrected. A wait ends when either says so, so neither a sleep nor a correction stretches it.

/** The longest one timer runs: a long wait is taken in steps, so a machine that slept is seen within a step of
 * waking. */
export const STEP = 60;

/** The two clocks, in milliseconds, passed in so tests can move each on its own. */
export interface Clocks {
  wall: () => number;
  steady: () => number;
}

const BROWSER: Clocks = { wall: () => Date.now(), steady: () => performance.now() };

/** A moment some seconds from now, on both clocks. */
export class Deadline {
  private readonly wall: number;
  private readonly steady: number;

  constructor(
    seconds: number,
    private readonly clocks: Clocks = BROWSER,
  ) {
    this.wall = clocks.wall() + seconds * 1000;
    this.steady = clocks.steady() + seconds * 1000;
  }

  /** Milliseconds until it falls, by whichever clock is nearer to it; zero or less once it has. */
  left(): number {
    return Math.min(this.wall - this.clocks.wall(), this.steady - this.clocks.steady());
  }
}

/** One pending timer at a time: setting it again replaces what was set. */
export class Timer {
  private pending: ReturnType<typeof setTimeout> | undefined;

  constructor(private readonly clocks: Clocks = BROWSER) {}

  /** Runs `then` once `seconds` have passed by either clock, through timers of at most a step each. */
  after(seconds: number, then: () => void): void {
    this.stop();
    const due = new Deadline(seconds, this.clocks);
    const step = (): void => {
      const wait = due.left();
      if (wait <= 0) {
        then();
        return;
      }
      this.pending = setTimeout(step, Math.min(wait, STEP * 1000));
    };
    step();
  }

  stop(): void {
    clearTimeout(this.pending);
  }
}
