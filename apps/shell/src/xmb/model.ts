/**
 * The XMB navigation state machine.
 *
 * Kept free of DOM and platform access so the cross-media-bar's behaviour can be
 * tested directly. The renderer is a pure function of the state this produces.
 *
 * Faithfulness notes, since these are the details that make it feel like a PSP
 * rather than a generic menu:
 *
 * - The cursor never wraps. Pressing Up on the first item does nothing; the real
 *   XMB stops dead at the ends of a column.
 * - Each category remembers its own cursor position, so returning to Game lands
 *   back on the game you were looking at.
 * - Confirming an item with children pushes a new column that slides in from the
 *   right; Back pops it. Categories are only reachable from the root column.
 * - Moving sideways while deep in a submenu is not possible — the real XMB traps
 *   horizontal input until you back out.
 */

export type ItemKind = 'game' | 'action' | 'submenu' | 'info';

export interface XmbItem {
  id: string;
  label: string;
  /** Second line, e.g. a disc ID or "1.2 GB · ISO". */
  sublabel?: string;
  kind: ItemKind;
  /** `data:` URL for a game's ICON0, or a built-in glyph name. */
  icon?: string;
  /** 480×272 background shown behind this entry when selected. */
  background?: string;
  children?: XmbItem[];
  /** Opaque payload the host acts on — for a game, its library entry. */
  payload?: unknown;
  /** Rendered greyed out and refuses to activate. */
  disabled?: boolean;
}

export interface XmbCategory {
  id: string;
  label: string;
  /** Built-in glyph name drawn for the category icon. */
  glyph: string;
  items: XmbItem[];
}

/** One open vertical column. The root column is the selected category's items. */
export interface XmbColumn {
  /** Item whose children this column shows; absent for the root column. */
  parent?: XmbItem;
  items: XmbItem[];
  cursor: number;
}

export interface XmbState {
  categories: XmbCategory[];
  categoryIndex: number;
  /** Innermost column is last. Always at least one entry. */
  columns: XmbColumn[];
  /** Remembered cursor per category id, so switching back restores position. */
  memory: Record<string, number>;
}

export type XmbInput = 'up' | 'down' | 'left' | 'right' | 'confirm' | 'back';

/** Something the host must act on — launching a game, running an action. */
export type XmbEffect =
  | { type: 'launch'; item: XmbItem }
  | { type: 'action'; item: XmbItem }
  | { type: 'blocked'; item: XmbItem };

export interface XmbTransition {
  state: XmbState;
  /** Distinguishes "cursor moved" from "input hit a wall", which drives the sound. */
  moved: boolean;
  effect?: XmbEffect;
}

export function createState(categories: XmbCategory[], categoryIndex = 0): XmbState {
  const safeIndex = clamp(categoryIndex, 0, Math.max(0, categories.length - 1));
  return {
    categories,
    categoryIndex: safeIndex,
    columns: [{ items: categories[safeIndex]?.items ?? [], cursor: 0 }],
    memory: {},
  };
}

export function currentCategory(state: XmbState): XmbCategory | undefined {
  return state.categories[state.categoryIndex];
}

export function currentColumn(state: XmbState): XmbColumn {
  return state.columns[state.columns.length - 1];
}

export function currentItem(state: XmbState): XmbItem | undefined {
  const column = currentColumn(state);
  return column.items[column.cursor];
}

/** Depth 0 is the category's own item list. */
export function depth(state: XmbState): number {
  return state.columns.length - 1;
}

export function reduce(state: XmbState, input: XmbInput): XmbTransition {
  switch (input) {
    case 'up':
      return moveCursor(state, -1);
    case 'down':
      return moveCursor(state, +1);
    case 'left':
      return moveCategory(state, -1);
    case 'right':
      return moveCategory(state, +1);
    case 'confirm':
      return confirm(state);
    case 'back':
      return back(state);
  }
}

function moveCursor(state: XmbState, delta: number): XmbTransition {
  const column = currentColumn(state);
  const next = column.cursor + delta;
  // No wrapping: the XMB stops at the ends of a column.
  if (next < 0 || next >= column.items.length) {
    return { state, moved: false };
  }

  const columns = state.columns.slice();
  columns[columns.length - 1] = { ...column, cursor: next };

  // Only the root column's position is worth remembering; submenus reopen at the top.
  const memory =
    columns.length === 1 && currentCategory(state)
      ? { ...state.memory, [currentCategory(state)!.id]: next }
      : state.memory;

  return { state: { ...state, columns, memory }, moved: true };
}

function moveCategory(state: XmbState, delta: number): XmbTransition {
  // Horizontal input is trapped while a submenu is open, matching the real XMB.
  if (depth(state) > 0) {
    return { state, moved: false };
  }
  const next = state.categoryIndex + delta;
  if (next < 0 || next >= state.categories.length) {
    return { state, moved: false };
  }

  const category = state.categories[next];
  const remembered = state.memory[category.id] ?? 0;
  return {
    state: {
      ...state,
      categoryIndex: next,
      columns: [
        {
          items: category.items,
          cursor: clamp(remembered, 0, Math.max(0, category.items.length - 1)),
        },
      ],
    },
    moved: true,
  };
}

function confirm(state: XmbState): XmbTransition {
  const item = currentItem(state);
  if (!item) {
    return { state, moved: false };
  }
  if (item.disabled) {
    return { state, moved: false, effect: { type: 'blocked', item } };
  }

  switch (item.kind) {
    case 'submenu': {
      const children = item.children ?? [];
      // An empty submenu would open a dead column with nothing to back out of.
      if (children.length === 0) {
        return { state, moved: false, effect: { type: 'blocked', item } };
      }
      return {
        state: {
          ...state,
          columns: [...state.columns, { parent: item, items: children, cursor: 0 }],
        },
        moved: true,
      };
    }
    case 'game':
      return { state, moved: true, effect: { type: 'launch', item } };
    case 'action':
      return { state, moved: true, effect: { type: 'action', item } };
    case 'info':
      return { state, moved: false };
  }
}

function back(state: XmbState): XmbTransition {
  if (state.columns.length <= 1) {
    return { state, moved: false };
  }
  return { state: { ...state, columns: state.columns.slice(0, -1) }, moved: true };
}

/**
 * Swaps in a rebuilt category — used when a library rescan finishes — without
 * disturbing where the user is looking.
 */
export function replaceCategoryItems(
  state: XmbState,
  categoryId: string,
  items: XmbItem[],
): XmbState {
  const index = state.categories.findIndex((c) => c.id === categoryId);
  if (index === -1) {
    return state;
  }

  const categories = state.categories.slice();
  categories[index] = { ...categories[index], items };

  // Only the visible column needs re-clamping, and only if it is the root column
  // of the category that changed. A user reading a submenu keeps their place.
  let columns = state.columns;
  if (index === state.categoryIndex && state.columns.length === 1) {
    columns = [
      { items, cursor: clamp(state.columns[0].cursor, 0, Math.max(0, items.length - 1)) },
    ];
  }

  return { ...state, categories, columns };
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}
