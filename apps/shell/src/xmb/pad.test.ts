/**
 * Controller identification and mapping tests.
 *
 * Built around synthetic `Gamepad` objects using the `id` strings and layouts
 * real hardware actually reports, so the awkward cases — a DualShock 4 in
 * non-standard mode with a hat-axis D-pad, WebKit exposing no vendor IDs — are
 * pinned rather than assumed.
 */

import { describe, expect, it } from 'vitest';

import {
  faceGlyphs,
  hatDirections,
  identifyController,
  parseGamepadId,
  readPad,
  rumble,
  stickDirections,
  type PadDirection,
} from './pad';

/** Builds a Gamepad-shaped object; only the fields this module reads matter. */
function gamepad(options: {
  id: string;
  mapping?: GamepadMappingType | '';
  buttons?: number[];
  axes?: number[];
  buttonCount?: number;
  axisCount?: number;
}): Gamepad {
  const buttonCount = options.buttonCount ?? 18;
  const axisCount = options.axisCount ?? 10;
  const pressedIndices = new Set(options.buttons ?? []);

  return {
    id: options.id,
    index: 0,
    connected: true,
    timestamp: 0,
    mapping: (options.mapping ?? 'standard') as GamepadMappingType,
    axes: Array.from({ length: axisCount }, (_, i) => options.axes?.[i] ?? 0),
    buttons: Array.from({ length: buttonCount }, (_, i) => ({
      pressed: pressedIndices.has(i),
      touched: pressedIndices.has(i),
      value: pressedIndices.has(i) ? 1 : 0,
    })),
    vibrationActuator: null,
  } as unknown as Gamepad;
}

// Real id strings, as each engine reports them.
const CHROME_DUALSENSE =
  'Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)';
const CHROME_DS4 = 'Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 09cc)';
const CHROME_XBOX_SERIES = 'Xbox Wireless Controller (STANDARD GAMEPAD Vendor: 045e Product: 0b13)';
const CHROME_XBOX_360 = 'Xbox 360 Controller (XInput STANDARD GAMEPAD)';
const FIREFOX_DUALSENSE = '054c-0ce6-Wireless Controller';
const FIREFOX_DS4 = '054c-09cc-Wireless Controller';
const WEBKIT_DUALSENSE = 'DualSense Wireless Controller Extended Gamepad';
const WEBKIT_GENERIC_SONY = 'Wireless Controller Extended Gamepad';

describe('parsing the id string', () => {
  it('reads Chromium vendor and product ids', () => {
    expect(parseGamepadId(CHROME_DUALSENSE)).toEqual({
      vendorId: '054c',
      productId: '0ce6',
    });
  });

  it('reads the Firefox id format', () => {
    expect(parseGamepadId(FIREFOX_DS4)).toEqual({ vendorId: '054c', productId: '09cc' });
    expect(parseGamepadId(FIREFOX_DUALSENSE)).toEqual({ vendorId: '054c', productId: '0ce6' });
  });

  it('returns nothing for WebKit, which exposes no ids', () => {
    expect(parseGamepadId(WEBKIT_DUALSENSE)).toEqual({});
  });

  it('is case insensitive about hex and labels', () => {
    expect(parseGamepadId('Pad (Vendor: 054C Product: 0CE6)')).toEqual({
      vendorId: '054c',
      productId: '0ce6',
    });
  });
});

describe('identifying controllers', () => {
  it('recognises a DualSense on Chromium', () => {
    const info = identifyController(CHROME_DUALSENSE);
    expect(info.kind).toBe('dualsense');
    expect(info.name).toBe('DualSense Wireless Controller');
    expect(info.faceStyle).toBe('playstation');
  });

  it('recognises a DualShock 4', () => {
    expect(identifyController(CHROME_DS4).kind).toBe('dualshock4');
    expect(identifyController(FIREFOX_DS4).name).toBe('DualShock 4 (2nd gen)');
  });

  it('recognises Xbox pads', () => {
    expect(identifyController(CHROME_XBOX_SERIES).kind).toBe('xbox-series');
    expect(identifyController(CHROME_XBOX_SERIES).faceStyle).toBe('xbox');
    expect(identifyController(CHROME_XBOX_360).kind).toBe('xbox-360');
  });

  it('recognises a DualSense by name when no ids are exposed', () => {
    // Safari's case: name matching is the only thing available.
    const info = identifyController(WEBKIT_DUALSENSE);
    expect(info.kind).toBe('dualsense');
    expect(info.faceStyle).toBe('playstation');
  });

  it('treats a bare "Wireless Controller" as PlayStation', () => {
    // Sony's own product string for both the DS4 and DualSense, so falling back
    // to "generic" here would show the wrong glyphs on the most common pad.
    expect(identifyController(WEBKIT_GENERIC_SONY).faceStyle).toBe('playstation');
  });

  it('falls back to the family for an unknown Sony product id', () => {
    // A future revision should still read as PlayStation, not unknown.
    const info = identifyController('Pad (Vendor: 054c Product: ffff)');
    expect(info.faceStyle).toBe('playstation');
    expect(info.name).toBe('PlayStation Controller');
  });

  it('falls back to the family for an unknown Microsoft product id', () => {
    const info = identifyController('Pad (Vendor: 045e Product: ffff)');
    expect(info.faceStyle).toBe('xbox');
    expect(info.name).toBe('Xbox Controller');
  });

  it('reports an unrecognised pad as generic without throwing', () => {
    const info = identifyController('Some Third-Party Pad');
    expect(info.kind).toBe('generic');
    expect(info.name).toBe('Some Third-Party Pad');
  });

  it('survives an empty id', () => {
    expect(identifyController('').name).toBe('Controller');
  });
});

describe('face glyphs', () => {
  it('shows PlayStation and Xbox labels for their own pads', () => {
    expect(faceGlyphs('playstation')).toEqual({ confirm: '✕', back: '○' });
    expect(faceGlyphs('xbox')).toEqual({ confirm: 'A', back: 'B' });
  });

  it('defaults to PlayStation glyphs, matching the PSP', () => {
    expect(faceGlyphs('generic')).toEqual({ confirm: '✕', back: '○' });
  });
});

describe('reading a standard-mapping pad', () => {
  it('reads the D-pad buttons', () => {
    const pad = gamepad({ id: CHROME_DUALSENSE, buttons: [12] });
    expect([...readPad(pad, identifyController(pad.id)).directions]).toEqual(['up']);
  });

  it('reads confirm from the bottom face button', () => {
    const pad = gamepad({ id: CHROME_DUALSENSE, buttons: [0] });
    const snapshot = readPad(pad, identifyController(pad.id));
    expect(snapshot.confirm).toBe(true);
    expect(snapshot.back).toBe(false);
  });

  it('reads back from the right face button', () => {
    const pad = gamepad({ id: CHROME_DUALSENSE, buttons: [1] });
    expect(readPad(pad, identifyController(pad.id)).back).toBe(true);
  });

  it('maps the same indices for an Xbox pad', () => {
    // A is index 0 like ✕, which is why confirm needs no per-family special case.
    const pad = gamepad({ id: CHROME_XBOX_SERIES, buttons: [0, 13] });
    const snapshot = readPad(pad, identifyController(pad.id));
    expect(snapshot.confirm).toBe(true);
    expect([...snapshot.directions]).toEqual(['down']);
  });

  it('treats a high analog value as pressed', () => {
    // Some drivers report a value without ever setting `pressed`.
    const pad = gamepad({ id: CHROME_DUALSENSE });
    (pad.buttons[0] as { value: number }).value = 0.9;
    expect(readPad(pad, identifyController(pad.id)).confirm).toBe(true);
  });
});

describe('reading a non-standard DualShock 4', () => {
  // The case that breaks naive implementations: over Bluetooth on some
  // platforms the DS4 reports no standard mapping, Sony's raw button order, and
  // its D-pad on a hat axis.
  const info = identifyController(FIREFOX_DS4);

  it('reads confirm from index 1, not 0', () => {
    const pad = gamepad({ id: FIREFOX_DS4, mapping: '', buttons: [1] });
    const snapshot = readPad(pad, info);
    expect(snapshot.confirm).toBe(true);
    expect(snapshot.back).toBe(false);
  });

  it('reads back from index 2', () => {
    const pad = gamepad({ id: FIREFOX_DS4, mapping: '', buttons: [2] });
    expect(readPad(pad, info).back).toBe(true);
  });

  it('does not mistake Square for confirm', () => {
    // Index 0 is Square in Sony's raw order; treating it as ✕ would launch games
    // on the wrong button.
    const pad = gamepad({ id: FIREFOX_DS4, mapping: '', buttons: [0] });
    const snapshot = readPad(pad, info);
    expect(snapshot.confirm).toBe(false);
    expect(snapshot.back).toBe(false);
  });

  it('reads the D-pad from the hat axis', () => {
    const pad = gamepad({ id: FIREFOX_DS4, mapping: '', axes: [0, 0, 0, 0, 0, 0, 0, 0, 0, -1] });
    expect([...readPad(pad, info).directions]).toEqual(['up']);
  });

  it('falls back to standard face indices for an unknown non-standard pad', () => {
    const generic = identifyController('Some Third-Party Pad');
    const pad = gamepad({ id: 'Some Third-Party Pad', mapping: '', buttons: [0] });
    expect(readPad(pad, generic).confirm).toBe(true);
  });
});

describe('the hat axis', () => {
  it('decodes all eight positions', () => {
    const step = 2 / 7;
    const expected: PadDirection[][] = [
      ['up'],
      ['up', 'right'],
      ['right'],
      ['right', 'down'],
      ['down'],
      ['down', 'left'],
      ['left'],
      ['left', 'up'],
    ];
    expected.forEach((directions, index) => {
      const value = -1 + index * step;
      expect(hatDirections(axesWithHat(value))).toEqual(directions);
    });
  });

  it('reports nothing when centred', () => {
    // Centred is encoded *outside* the -1..1 range. Reading it as a direction is
    // what makes a D-pad appear permanently pressed left.
    expect(hatDirections(axesWithHat(1.2857142857142856))).toEqual([]);
    expect(hatDirections(axesWithHat(3.2857142857142856))).toEqual([]);
  });

  it('ignores a pad with no hat axis at all', () => {
    expect(hatDirections([0, 0])).toEqual([]);
  });

  it('tolerates small floating-point drift around a position', () => {
    expect(hatDirections(axesWithHat(-0.999))).toEqual(['up']);
    expect(hatDirections(axesWithHat(0.1429))).toEqual(['down']);
  });

  function axesWithHat(value: number): number[] {
    const axes = new Array(10).fill(0);
    axes[9] = value;
    return axes;
  }
});

describe('the analog stick', () => {
  it('registers a clear push', () => {
    expect(stickDirections(0, -1)).toEqual(['up']);
    expect(stickDirections(1, 0)).toEqual(['right']);
  });

  it('ignores drift inside the deadzone', () => {
    expect(stickDirections(0.2, -0.15)).toEqual([]);
  });

  it('reports diagonals as both directions', () => {
    expect(stickDirections(-0.8, -0.8).sort()).toEqual(['left', 'up']);
  });

  it('keeps a held direction past the release threshold', () => {
    // Hysteresis: 0.45 is below the press threshold but above the release one, so
    // an already-held direction stays held instead of flickering.
    expect(stickDirections(0, -0.45, new Set())).toEqual([]);
    expect(stickDirections(0, -0.45, new Set<PadDirection>(['up']))).toEqual(['up']);
  });

  it('releases a held direction once the stick is near centre', () => {
    expect(stickDirections(0, -0.2, new Set<PadDirection>(['up']))).toEqual([]);
  });

  it('reads the stick through readPad', () => {
    const pad = gamepad({ id: CHROME_DUALSENSE, axes: [0, 1] });
    expect([...readPad(pad, identifyController(pad.id)).directions]).toEqual(['down']);
  });

  it('combines the stick and D-pad without duplicating a direction', () => {
    const pad = gamepad({ id: CHROME_DUALSENSE, buttons: [13], axes: [0, 1] });
    expect([...readPad(pad, identifyController(pad.id)).directions]).toEqual(['down']);
  });
});

describe('overrides imported from PPSSPP', () => {
  const info = identifyController(CHROME_DUALSENSE);

  it('adds a remapped confirm button', () => {
    // Someone who bound ✕ to the top face button in PPSSPP gets that button in
    // the XMB too.
    const pad = gamepad({ id: CHROME_DUALSENSE, buttons: [3] });
    expect(readPad(pad, info, new Set(), { confirm: [3] }).confirm).toBe(true);
  });

  it('keeps the built-in mapping working alongside an override', () => {
    // The safety property: importing a config must never take away a button that
    // already worked, or a wrong controls.ini would break a fine controller.
    const pad = gamepad({ id: CHROME_DUALSENSE, buttons: [0] });
    expect(readPad(pad, info, new Set(), { confirm: [3] }).confirm).toBe(true);
  });

  it('adds remapped directions', () => {
    const pad = gamepad({ id: CHROME_DUALSENSE, buttons: [4] });
    expect([...readPad(pad, info, new Set(), { down: [4] }).directions]).toEqual(['down']);
  });

  it('leaves the D-pad working when directions are overridden', () => {
    const pad = gamepad({ id: CHROME_DUALSENSE, buttons: [13] });
    expect([...readPad(pad, info, new Set(), { down: [4] }).directions]).toEqual(['down']);
  });

  it('adds a remapped back button', () => {
    const pad = gamepad({ id: CHROME_DUALSENSE, buttons: [2] });
    expect(readPad(pad, info, new Set(), { back: [2] }).back).toBe(true);
  });

  it('ignores an override index the pad does not have', () => {
    // A config naming a button this pad lacks must not throw or report a press.
    const pad = gamepad({ id: CHROME_DUALSENSE, buttonCount: 4 });
    const snapshot = readPad(pad, info, new Set(), { confirm: [17] });
    expect(snapshot.confirm).toBe(false);
  });

  it('an empty override set behaves exactly like no overrides', () => {
    const pad = gamepad({ id: CHROME_DUALSENSE, buttons: [0] });
    expect(readPad(pad, info, new Set(), {})).toEqual(readPad(pad, info));
  });

  it('works on a non-standard pad too', () => {
    // Overrides layer over the Sony raw ordering, not just the standard mapping.
    const ds4 = identifyController(FIREFOX_DS4);
    const pad = gamepad({ id: FIREFOX_DS4, mapping: '', buttons: [1] });
    expect(readPad(pad, ds4, new Set(), { confirm: [5] }).confirm).toBe(true);
  });
});

describe('rumble', () => {
  it('does nothing when the pad has no actuator', () => {
    // Must never throw: haptics are a flourish, not a requirement.
    expect(() => rumble(gamepad({ id: CHROME_DUALSENSE }))).not.toThrow();
  });

  it('plays a dual-rumble effect when supported', () => {
    const calls: Array<[string, Record<string, number>]> = [];
    const pad = gamepad({ id: CHROME_DUALSENSE });
    Object.assign(pad, {
      vibrationActuator: {
        playEffect: (type: string, params: Record<string, number>) => {
          calls.push([type, params]);
          return Promise.resolve('complete');
        },
      },
    });

    rumble(pad, { duration: 50, strong: 0.5, weak: 0.1 });
    expect(calls).toHaveLength(1);
    expect(calls[0][0]).toBe('dual-rumble');
    expect(calls[0][1]).toMatchObject({ duration: 50, strongMagnitude: 0.5, weakMagnitude: 0.1 });
  });

  it('swallows a rejected effect', () => {
    // Some drivers reject instead of reporting no support; an unhandled rejection
    // would surface as a console error on every navigation.
    const pad = gamepad({ id: CHROME_DUALSENSE });
    Object.assign(pad, {
      vibrationActuator: { playEffect: () => Promise.reject(new Error('unsupported')) },
    });
    expect(() => rumble(pad)).not.toThrow();
  });

  it('swallows an actuator that throws synchronously', () => {
    const pad = gamepad({ id: CHROME_DUALSENSE });
    Object.assign(pad, {
      vibrationActuator: {
        playEffect: () => {
          throw new Error('unknown effect type');
        },
      },
    });
    expect(() => rumble(pad)).not.toThrow();
  });
});
