/**
 * Controller identification and button mapping.
 *
 * The Gamepad API is only *mostly* uniform. A DualSense on Chrome reports
 * `mapping: "standard"` and everything lines up; the same pad over Bluetooth on
 * another browser or platform can report `mapping: ""` with Sony's raw HID
 * ordering and the D-pad encoded as a hat on `axes[9]`. Assuming the standard
 * layout is why "my PS4 controller's D-pad does nothing" is such a common bug.
 *
 * So this module does three things the raw API will not:
 *
 * 1. Identifies the pad from its `id` string, which is formatted differently by
 *    every browser, so the UI can name it and show the right face-button glyphs.
 * 2. Reads directions and face buttons through a per-device profile, covering
 *    standard mapping, Sony's raw ordering, and hat-axis D-pads.
 * 3. Applies deadzone hysteresis, so a stick resting near the threshold does not
 *    oscillate between pressed and released and double-step the cursor.
 *
 * Kept free of DOM and timers so all of it can be tested against synthetic
 * `Gamepad` objects.
 */

export type ControllerKind =
  | 'dualsense'
  | 'dualshock4'
  | 'xbox-series'
  | 'xbox-one'
  | 'xbox-360'
  | 'switch-pro'
  | 'generic';

/** Which glyphs to show for confirm/back. */
export type FaceStyle = 'playstation' | 'xbox' | 'nintendo' | 'generic';

export interface ControllerInfo {
  kind: ControllerKind;
  /** Name for the UI, e.g. "DualSense Wireless Controller". */
  name: string;
  faceStyle: FaceStyle;
  /** Lower-case hex, when the browser exposes it. */
  vendorId?: string;
  productId?: string;
}

/** USB vendor IDs, lower-case hex. */
const VENDOR_SONY = '054c';
const VENDOR_MICROSOFT = '045e';
const VENDOR_NINTENDO = '057e';

/**
 * Product IDs worth distinguishing by name.
 *
 * Not exhaustive — anything unlisted still resolves to the right family via its
 * vendor ID, so a new revision degrades to "PlayStation Controller" rather than
 * to "generic".
 */
const SONY_PRODUCTS: Record<string, { kind: ControllerKind; name: string }> = {
  '0ce6': { kind: 'dualsense', name: 'DualSense Wireless Controller' },
  '0df2': { kind: 'dualsense', name: 'DualSense Edge Controller' },
  '05c4': { kind: 'dualshock4', name: 'DualShock 4' },
  '09cc': { kind: 'dualshock4', name: 'DualShock 4 (2nd gen)' },
  '0ba0': { kind: 'dualshock4', name: 'DualShock 4 (USB adapter)' },
};

const MICROSOFT_PRODUCTS: Record<string, { kind: ControllerKind; name: string }> = {
  '0b12': { kind: 'xbox-series', name: 'Xbox Wireless Controller' },
  '0b13': { kind: 'xbox-series', name: 'Xbox Wireless Controller' },
  '0b20': { kind: 'xbox-series', name: 'Xbox Wireless Controller' },
  '02ea': { kind: 'xbox-one', name: 'Xbox One Controller' },
  '02fd': { kind: 'xbox-one', name: 'Xbox One Controller' },
  '02dd': { kind: 'xbox-one', name: 'Xbox One Controller' },
  '028e': { kind: 'xbox-360', name: 'Xbox 360 Controller' },
};

/**
 * Pulls vendor and product IDs out of a `Gamepad.id`.
 *
 * Each engine formats it differently:
 * - Chromium: `Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)`
 * - Firefox:  `054c-0ce6-Wireless Controller`
 * - WebKit:   `Wireless Controller Extended Gamepad` — no IDs at all
 *
 * Returns `undefined` ids for the WebKit case, which is why identification also
 * falls back to matching the name.
 */
export function parseGamepadId(id: string): { vendorId?: string; productId?: string } {
  const chromium = /vendor:\s*([0-9a-f]{4}).*?product:\s*([0-9a-f]{4})/i.exec(id);
  if (chromium) {
    return { vendorId: chromium[1].toLowerCase(), productId: chromium[2].toLowerCase() };
  }

  const firefox = /^([0-9a-f]{4})-([0-9a-f]{4})/i.exec(id);
  if (firefox) {
    return { vendorId: firefox[1].toLowerCase(), productId: firefox[2].toLowerCase() };
  }

  return {};
}

/** Identifies a controller from its `Gamepad.id`. */
export function identifyController(id: string): ControllerInfo {
  const { vendorId, productId } = parseGamepadId(id);
  const lower = id.toLowerCase();

  if (vendorId === VENDOR_SONY) {
    const known = productId ? SONY_PRODUCTS[productId] : undefined;
    return {
      kind: known?.kind ?? 'dualshock4',
      name: known?.name ?? 'PlayStation Controller',
      faceStyle: 'playstation',
      vendorId,
      productId,
    };
  }

  if (vendorId === VENDOR_MICROSOFT) {
    const known = productId ? MICROSOFT_PRODUCTS[productId] : undefined;
    return {
      kind: known?.kind ?? 'xbox-series',
      name: known?.name ?? 'Xbox Controller',
      faceStyle: 'xbox',
      vendorId,
      productId,
    };
  }

  if (vendorId === VENDOR_NINTENDO) {
    return {
      kind: 'switch-pro',
      name: 'Nintendo Pro Controller',
      faceStyle: 'nintendo',
      vendorId,
      productId,
    };
  }

  // No usable IDs — WebKit, or a pad behind an adapter. Match on the name, which
  // is all Safari gives us.
  if (lower.includes('dualsense')) {
    return { kind: 'dualsense', name: 'DualSense Wireless Controller', faceStyle: 'playstation' };
  }
  if (lower.includes('dualshock') || lower.includes('playstation')) {
    return { kind: 'dualshock4', name: 'DualShock 4', faceStyle: 'playstation' };
  }
  // Chromium reports XInput pads as "Xbox 360 Controller (XInput STANDARD
  // GAMEPAD)" with no ids at all, so the generation has to come from the name.
  // Check the specific generations before the bare "xbox".
  if (lower.includes('xbox 360')) {
    return { kind: 'xbox-360', name: 'Xbox 360 Controller', faceStyle: 'xbox' };
  }
  if (lower.includes('xbox one')) {
    return { kind: 'xbox-one', name: 'Xbox One Controller', faceStyle: 'xbox' };
  }
  if (lower.includes('xbox')) {
    return { kind: 'xbox-series', name: 'Xbox Controller', faceStyle: 'xbox' };
  }
  // Sony's own name for the DS4 and DualSense is the unhelpfully generic
  // "Wireless Controller", so treat it as PlayStation rather than unknown.
  if (lower.includes('wireless controller')) {
    return { kind: 'dualshock4', name: 'PlayStation Controller', faceStyle: 'playstation' };
  }

  return { kind: 'generic', name: id || 'Controller', faceStyle: 'generic' };
}

/** Glyphs for the on-screen hints, per family. */
export function faceGlyphs(style: FaceStyle): { confirm: string; back: string } {
  switch (style) {
    case 'playstation':
      return { confirm: '✕', back: '○' };
    case 'xbox':
      return { confirm: 'A', back: 'B' };
    case 'nintendo':
      // Nintendo's physical A is on the right, where PlayStation's ○ sits, but the
      // Gamepad API reports the bottom button as index 0 either way — so the
      // glyph shown is the one the button is labelled with on that pad.
      return { confirm: 'B', back: 'A' };
    default:
      return { confirm: '✕', back: '○' };
  }
}

// --- Reading ---------------------------------------------------------------

export type PadDirection = 'up' | 'down' | 'left' | 'right';

export interface PadSnapshot {
  directions: Set<PadDirection>;
  confirm: boolean;
  back: boolean;
}

/**
 * Stick thresholds.
 *
 * Press and release differ on purpose: with a single threshold, a stick left
 * resting just past it flickers, and each flicker is another cursor step. The
 * gap means a direction must be clearly released before it can fire again.
 */
const STICK_PRESS = 0.6;
const STICK_RELEASE = 0.38;

/**
 * Face-button indices for pads that do not report the standard mapping.
 *
 * Sony's raw HID order puts Square first and Cross second, so a pad in
 * non-standard mode has confirm at index 1, not 0.
 */
const NON_STANDARD_FACE: Partial<Record<ControllerKind, { confirm: number; back: number }>> = {
  dualsense: { confirm: 1, back: 2 },
  dualshock4: { confirm: 1, back: 2 },
};

/** Standard mapping: index 0 is the bottom face button, 1 the right one. */
const STANDARD_FACE = { confirm: 0, back: 1 };

/**
 * Extra button indices imported from PPSSPP's own `controls.ini`.
 *
 * Keyed by action name, matching `psp_host::PadProfile`.
 */
export type PadOverrides = Readonly<Record<string, readonly number[]>>;

/**
 * Reads a gamepad into logical state.
 *
 * `held` is the previous frame's directions, needed for the release threshold.
 *
 * `overrides` carries whatever PPSSPP has configured. It is applied *in addition
 * to* the built-in mapping, never instead of it: a config written for a different
 * pad, or one whose keycodes this build does not recognise, must not be able to
 * take away a button that already worked. The cost of the union is that an
 * unbound button might still act — far better than a dead ✕.
 */
export function readPad(
  pad: Gamepad,
  info: ControllerInfo,
  held: ReadonlySet<PadDirection> = new Set(),
  overrides: PadOverrides = {},
): PadSnapshot {
  const standard = pad.mapping === 'standard';
  const face = standard ? STANDARD_FACE : NON_STANDARD_FACE[info.kind] ?? STANDARD_FACE;

  const directions = new Set<PadDirection>();

  // D-pad as buttons: the standard mapping's 12-15, which many non-standard pads
  // also happen to expose.
  if (pressed(pad, 12)) directions.add('up');
  if (pressed(pad, 13)) directions.add('down');
  if (pressed(pad, 14)) directions.add('left');
  if (pressed(pad, 15)) directions.add('right');

  // Directions PPSSPP has bound, on top of the above.
  for (const direction of ['up', 'down', 'left', 'right'] as const) {
    if (anyPressed(pad, overrides[direction])) {
      directions.add(direction);
    }
  }

  // D-pad as a hat axis. Non-standard DS4 and DualSense report it on axes[9],
  // and without this their D-pad appears completely dead.
  //
  // Only consulted for non-standard pads: the standard mapping guarantees the
  // D-pad is on buttons 12-15, and plenty of standard pads still expose ten axes
  // with axes[9] sitting at 0 — which is not a valid hat position and must not be
  // read as one.
  if (!standard) {
    for (const direction of hatDirections(pad.axes)) {
      directions.add(direction);
    }
  }

  // Left stick, with hysteresis against the previous frame.
  for (const direction of stickDirections(pad.axes[0] ?? 0, pad.axes[1] ?? 0, held)) {
    directions.add(direction);
  }

  return {
    directions,
    confirm: pressed(pad, face.confirm) || anyPressed(pad, overrides.confirm),
    back: pressed(pad, face.back) || anyPressed(pad, overrides.back),
  };
}

function anyPressed(pad: Gamepad, indices: readonly number[] | undefined): boolean {
  return indices?.some((index) => pressed(pad, index)) ?? false;
}

function pressed(pad: Gamepad, index: number): boolean {
  const button = pad.buttons[index];
  if (!button) {
    return false;
  }
  // Analog triggers report a value without setting `pressed` on some drivers.
  return button.pressed || button.value > 0.5;
}

/**
 * Decodes an SDL-style hat axis into directions.
 *
 * The eight positions are encoded as steps of 2/7 from -1 (up) clockwise to
 * 1 (up-left). Centred is reported *outside* that range — commonly 1.2857 or
 * 3.2857 — so anything past 1 is "nothing pressed", which is what makes a naive
 * `value > 0` check register a permanent left-press.
 *
 * A value must land near one of those eight positions to count. Note that 0 is
 * not among them: it falls exactly between the two "right" steps, so an idle axis
 * sitting at 0 must read as centred rather than being rounded into a direction.
 */
export function hatDirections(axes: readonly number[]): PadDirection[] {
  const value = axes[9];
  if (value === undefined || value > 1.05 || value < -1.05) {
    return [];
  }

  // Nearest of the eight positions, each 2/7 apart starting at -1.
  const step = 2 / 7;
  const index = Math.round((value + 1) / step);
  const positions: PadDirection[][] = [
    ['up'],
    ['up', 'right'],
    ['right'],
    ['right', 'down'],
    ['down'],
    ['down', 'left'],
    ['left'],
    ['left', 'up'],
  ];

  // Reject anything not actually close to that position — an unused axis reading
  // 0 is 0.143 away from its nearest neighbour and so is not a D-pad press.
  const nearest = -1 + index * step;
  if (Math.abs(value - nearest) > 0.1) {
    return [];
  }

  return positions[index] ?? [];
}

/** Converts stick axes into directions, using the release threshold for held ones. */
export function stickDirections(
  x: number,
  y: number,
  held: ReadonlySet<PadDirection> = new Set(),
): PadDirection[] {
  const out: PadDirection[] = [];
  const threshold = (direction: PadDirection) =>
    held.has(direction) ? STICK_RELEASE : STICK_PRESS;

  if (y <= -threshold('up')) out.push('up');
  if (y >= threshold('down')) out.push('down');
  if (x <= -threshold('left')) out.push('left');
  if (x >= threshold('right')) out.push('right');
  return out;
}

// --- Haptics ---------------------------------------------------------------

/**
 * The haptics actuator, as engines that support it actually shape it.
 *
 * Not written as `extends Gamepad`: the DOM lib types `vibrationActuator` as
 * non-optional, while several engines omit it entirely, so the two declarations
 * conflict. A structural view avoids claiming it is always there.
 */
interface HapticActuator {
  playEffect?(type: string, params: Record<string, number>): Promise<string>;
}

/**
 * Fires a short rumble, if the pad and engine support it.
 *
 * DualSense, DualShock 4 and Xbox pads all handle `dual-rumble` in Chromium.
 * Feature-detected and failure-swallowed on purpose: haptics are a flourish, and
 * an unsupported pad or a rejected promise must never interrupt navigation.
 */
export function rumble(
  pad: Gamepad,
  options: { duration?: number; strong?: number; weak?: number } = {},
): void {
  const actuator = (pad as { vibrationActuator?: HapticActuator }).vibrationActuator;
  if (!actuator?.playEffect) {
    return;
  }
  try {
    void actuator
      .playEffect('dual-rumble', {
        startDelay: 0,
        duration: options.duration ?? 80,
        strongMagnitude: options.strong ?? 0.35,
        weakMagnitude: options.weak ?? 0.2,
      })
      .catch(() => {
        // Some drivers reject rather than reporting no support.
      });
  } catch {
    // Older implementations throw on an unknown effect type.
  }
}
