/**
 * Input routing for keyboard and controllers.
 *
 * Both sources feed one repeat model so a held D-pad and a held arrow key scroll
 * at exactly the same rate. The browser's own key-repeat is deliberately ignored:
 * its rate is an OS preference, and the XMB's cadence is part of its feel.
 *
 * Controller reading lives in `pad.ts`, which handles the awkward parts — pads
 * that report a non-standard mapping, D-pads encoded as a hat axis, deadzone
 * hysteresis — so this file only deals with edges and timing.
 */

import {
  identifyController,
  readPad,
  rumble,
  type ControllerInfo,
  type PadDirection,
  type PadOverrides,
  type PadSnapshot,
} from './pad';
import type { XmbInput } from './model';

/** Delay before a held direction starts repeating. */
const REPEAT_DELAY_MS = 380;
/** Interval between repeats once started. */
const REPEAT_INTERVAL_MS = 110;

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

interface HeldInput {
  /** Timestamp of the next repeat emission. */
  nextAt: number;
  /** False until the first repeat fires, so the initial delay is longer. */
  repeating: boolean;
}

export interface ControllerChange {
  type: 'connected' | 'disconnected';
  controller: ControllerInfo;
  /** Everything still connected after the change. */
  connected: ControllerInfo[];
}

export interface InputRouterOptions {
  /** Set false to ignore controllers, e.g. in tests. */
  gamepad?: boolean;
  /** Called when a controller is plugged in, paired, or removed. */
  onControllerChange?: (change: ControllerChange) => void;
}

export class InputRouter {
  private readonly held = new Map<XmbInput, HeldInput>();
  private readonly emit: (input: XmbInput) => void;
  private readonly useGamepad: boolean;
  private readonly onControllerChange?: (change: ControllerChange) => void;
  private frame = 0;
  private detachers: Array<() => void> = [];

  /** Logical inputs that were down on the previous poll, for edge detection. */
  private padDown = new Set<XmbInput>();
  /** Previous frame's stick directions, so pad.ts can apply its release threshold. */
  private padDirections: ReadonlySet<PadDirection> = new Set();
  /** Identification cached per gamepad index; parsing the id string every frame is waste. */
  private identified = new Map<number, ControllerInfo>();
  /** Extra button indices imported from PPSSPP, applied on top of the defaults. */
  private overrides: PadOverrides = {};

  constructor(emit: (input: XmbInput) => void, options: InputRouterOptions = {}) {
    this.emit = emit;
    this.useGamepad = options.gamepad ?? true;
    this.onControllerChange = options.onControllerChange;
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

    const onGamepadConnected = (event: Event) => {
      const pad = (event as GamepadEvent).gamepad;
      const controller = identifyController(pad.id);
      this.identified.set(pad.index, controller);
      // A short buzz confirms the pad is actually talking to this app, which is
      // otherwise invisible when pairing over Bluetooth.
      rumble(pad, { duration: 120, strong: 0.4, weak: 0.25 });
      this.onControllerChange?.({
        type: 'connected',
        controller,
        connected: this.connectedControllers(),
      });
    };

    const onGamepadDisconnected = (event: Event) => {
      const pad = (event as GamepadEvent).gamepad;
      const controller = this.identified.get(pad.index) ?? identifyController(pad.id);
      this.identified.delete(pad.index);
      // Anything the pad was holding must be let go, or it repeats forever.
      this.releasePadInputs();
      this.onControllerChange?.({
        type: 'disconnected',
        controller,
        connected: this.connectedControllers(),
      });
    };

    target.addEventListener('keydown', onKeyDown);
    target.addEventListener('keyup', onKeyUp);
    target.addEventListener('blur', onBlur);
    target.addEventListener('gamepadconnected', onGamepadConnected);
    target.addEventListener('gamepaddisconnected', onGamepadDisconnected);

    this.detachers = [
      () => target.removeEventListener('keydown', onKeyDown),
      () => target.removeEventListener('keyup', onKeyUp),
      () => target.removeEventListener('blur', onBlur),
      () => target.removeEventListener('gamepadconnected', onGamepadConnected),
      () => target.removeEventListener('gamepaddisconnected', onGamepadDisconnected),
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
    this.padDirections = new Set();
  }

  /**
   * Controllers currently visible to the browser.
   *
   * Read live rather than from a cache: pads that were already connected when the
   * page loaded never fire a `gamepadconnected` event until they are touched.
   */
  connectedControllers(): ControllerInfo[] {
    const pads = navigator.getGamepads?.() ?? [];
    const out: ControllerInfo[] = [];
    for (const pad of pads) {
      if (!pad) {
        continue;
      }
      let info = this.identified.get(pad.index);
      if (!info) {
        info = identifyController(pad.id);
        this.identified.set(pad.index, info);
      }
      out.push(info);
    }
    return out;
  }

  /**
   * Adopts the mapping PPSSPP already has.
   *
   * Additive: these indices supplement the built-in mapping rather than replacing
   * it, so importing a config can only ever add working buttons.
   */
  setPadOverrides(overrides: PadOverrides): void {
    this.overrides = overrides;
  }

  /** Buzzes every connected pad, e.g. to acknowledge launching a game. */
  rumbleAll(options?: { duration?: number; strong?: number; weak?: number }): void {
    for (const pad of navigator.getGamepads?.() ?? []) {
      if (pad) {
        rumble(pad, options);
      }
    }
  }

  /**
   * Advances repeat timers and polls controllers.
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
    const directions = new Set<PadDirection>();

    for (const pad of pads) {
      if (!pad) {
        continue;
      }
      let info = this.identified.get(pad.index);
      if (!info) {
        // A pad already connected at load never fires the connect event, so
        // identify lazily here too.
        info = identifyController(pad.id);
        this.identified.set(pad.index, info);
      }

      const snapshot: PadSnapshot = readPad(pad, info, this.padDirections, this.overrides);
      for (const direction of snapshot.directions) {
        directions.add(direction);
        nowDown.add(direction);
      }
      if (snapshot.confirm) nowDown.add('confirm');
      if (snapshot.back) nowDown.add('back');
    }

    // Directions are tracked separately from `padDown` because hysteresis needs
    // last frame's *stick* state specifically, not every logical input.
    this.padDirections = directions;

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

  /** Releases everything a controller was holding, on disconnect. */
  private releasePadInputs(): void {
    for (const input of this.padDown) {
      this.release(input);
    }
    this.padDown.clear();
    this.padDirections = new Set();
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
