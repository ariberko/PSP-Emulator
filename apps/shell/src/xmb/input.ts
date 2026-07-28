/**
 * Input routing for keyboard and gamepad.
 *
 * Both sources feed one repeat model so a held D-pad and a held arrow key scroll
 * at exactly the same rate. The browser's own key-repeat is deliberately ignored:
 * its rate is an OS preference, and the XMB's cadence is part of its feel.
 */

import type { XmbInput } from './model';

/** Delay before a held direction starts repeating. */
const REPEAT_DELAY_MS = 380;
/** Interval between repeats once started. */
const REPEAT_INTERVAL_MS = 110;
/** How far the analog stick must move before it counts as a direction. */
const STICK_DEADZONE = 0.55;

/** Keyboard mapping. Z/X mirror the PSP's ○/✕ for players used to emulators. */
const KEY_MAP: Record<string, XmbInput> = {
  ArrowUp: 'up',
  ArrowDown: 'down',
  ArrowLeft: 'left',
  ArrowRight: 'right',
  KeyW: 'up',
  KeyS: 'down',
  KeyA: 'left',
  KeyD: 'right',
  Enter: 'confirm',
  Space: 'confirm',
  KeyX: 'confirm',
  Escape: 'back',
  Backspace: 'back',
  KeyZ: 'back',
};

/**
 * Standard Gamepad button indices.
 *
 * Index 0 is the bottom face button — ✕ on a DualShock, A on an Xbox pad — which
 * is confirm on a Western PSP. Index 1 is the right face button, ○/B, for back.
 */
const PAD_BUTTON_MAP: Record<number, XmbInput> = {
  0: 'confirm',
  1: 'back',
  12: 'up',
  13: 'down',
  14: 'left',
  15: 'right',
};

interface HeldInput {
  /** Timestamp of the next repeat emission. */
  nextAt: number;
  /** False until the first repeat fires, so the initial delay is longer. */
  repeating: boolean;
}

export interface InputRouterOptions {
  /** Set false to ignore gamepads, e.g. in tests. */
  gamepad?: boolean;
}

export class InputRouter {
  private readonly held = new Map<XmbInput, HeldInput>();
  private readonly emit: (input: XmbInput) => void;
  private readonly useGamepad: boolean;
  private frame = 0;
  private detachers: Array<() => void> = [];
  /** Buttons that were down on the previous poll, for edge detection. */
  private padDown = new Set<XmbInput>();

  constructor(emit: (input: XmbInput) => void, options: InputRouterOptions = {}) {
    this.emit = emit;
    this.useGamepad = options.gamepad ?? true;
  }

  attach(target: Window = window): void {
    const onKeyDown = (event: KeyboardEvent) => {
      const input = KEY_MAP[event.code];
      if (!input) {
        return;
      }
      // Stop the page from scrolling under the stage.
      event.preventDefault();
      // The browser's auto-repeat is discarded; the tick loop owns repeats.
      if (event.repeat) {
        return;
      }
      this.press(input);
    };

    const onKeyUp = (event: KeyboardEvent) => {
      const input = KEY_MAP[event.code];
      if (input) {
        this.release(input);
      }
    };

    // A window that loses focus mid-press would otherwise repeat forever.
    const onBlur = () => this.releaseAll();

    target.addEventListener('keydown', onKeyDown);
    target.addEventListener('keyup', onKeyUp);
    target.addEventListener('blur', onBlur);
    this.detachers = [
      () => target.removeEventListener('keydown', onKeyDown),
      () => target.removeEventListener('keyup', onKeyUp),
      () => target.removeEventListener('blur', onBlur),
    ];

    const loop = () => {
      this.tick(performance.now());
      this.frame = requestAnimationFrame(loop);
    };
    this.frame = requestAnimationFrame(loop);
  }

  detach(): void {
    cancelAnimationFrame(this.frame);
    this.detachers.forEach((off) => off());
    this.detachers = [];
    this.releaseAll();
  }

  /** Emits immediately and arms the repeat timer. */
  press(input: XmbInput, now = performance.now()): void {
    if (this.held.has(input)) {
      return;
    }
    this.held.set(input, { nextAt: now + REPEAT_DELAY_MS, repeating: false });
    this.emit(input);
  }

  release(input: XmbInput): void {
    this.held.delete(input);
  }

  releaseAll(): void {
    this.held.clear();
    this.padDown.clear();
  }

  /**
   * Advances repeat timers and polls gamepads.
   *
   * Exposed so tests can drive time forward without a real clock.
   */
  tick(now: number): void {
    if (this.useGamepad) {
      this.pollGamepads();
    }

    for (const [input, state] of this.held) {
      if (now < state.nextAt) {
        continue;
      }
      // Confirm and back must not auto-repeat — holding ✕ would launch a game
      // and then immediately re-trigger on the next screen.
      if (input === 'confirm' || input === 'back') {
        continue;
      }
      this.emit(input);
      state.repeating = true;
      state.nextAt = now + REPEAT_INTERVAL_MS;
    }
  }

  private pollGamepads(): void {
    const pads = navigator.getGamepads?.() ?? [];
    const nowDown = new Set<XmbInput>();

    for (const pad of pads) {
      if (!pad) {
        continue;
      }
      for (const [index, input] of Object.entries(PAD_BUTTON_MAP)) {
        if (pad.buttons[Number(index)]?.pressed) {
          nowDown.add(input);
        }
      }
      // Many pads report the D-pad as a hat on the axes instead of buttons, and
      // the analog stick should navigate too.
      const [x = 0, y = 0] = pad.axes;
      if (y <= -STICK_DEADZONE) nowDown.add('up');
      if (y >= STICK_DEADZONE) nowDown.add('down');
      if (x <= -STICK_DEADZONE) nowDown.add('left');
      if (x >= STICK_DEADZONE) nowDown.add('right');
    }

    for (const input of nowDown) {
      if (!this.padDown.has(input)) {
        this.press(input);
      }
    }
    for (const input of this.padDown) {
      if (!nowDown.has(input)) {
        this.release(input);
      }
    }
    this.padDown = nowDown;
  }
}

/** Exposed for the on-screen help overlay. */
export function keyBindings(): Array<{ keys: string[]; action: string }> {
  return [
    { keys: ['←', '→'], action: 'Change category' },
    { keys: ['↑', '↓'], action: 'Move through items' },
    { keys: ['Enter', 'X'], action: 'Confirm (✕)' },
    { keys: ['Esc', 'Z'], action: 'Back (○)' },
  ];
}
