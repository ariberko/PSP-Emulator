/**
 * Behavioural tests for the XMB state machine.
 *
 * These pin the details that make navigation feel like the console rather than a
 * generic list: no wrapping, per-category cursor memory, and horizontal input
 * being trapped inside a submenu.
 */

import { describe, expect, it } from 'vitest';

import {
  createState,
  currentColumn,
  currentItem,
  depth,
  reduce,
  replaceCategoryItems,
  type XmbCategory,
  type XmbInput,
  type XmbItem,
  type XmbState,
} from './model';

function game(id: string, label = id): XmbItem {
  return { id, label, kind: 'game', payload: { id } };
}

function categories(): XmbCategory[] {
  return [
    {
      id: 'settings',
      label: 'Settings',
      glyph: 'settings',
      items: [
        { id: 'sound', label: 'Sound', kind: 'action' },
        {
          id: 'controls',
          label: 'Controls',
          kind: 'submenu',
          children: [
            { id: 'dpad', label: 'D-Pad', kind: 'info' },
            { id: 'face', label: 'Face buttons', kind: 'info' },
          ],
        },
        { id: 'empty-menu', label: 'Empty', kind: 'submenu', children: [] },
        { id: 'locked', label: 'Locked', kind: 'action', disabled: true },
      ],
    },
    { id: 'game', label: 'Game', glyph: 'game', items: [game('a'), game('b'), game('c')] },
    { id: 'network', label: 'Network', glyph: 'network', items: [{ id: 'n1', label: 'Cloud', kind: 'action' }] },
  ];
}

/** Feeds a sequence of inputs and returns the final state. */
function run(state: XmbState, ...inputs: XmbInput[]): XmbState {
  return inputs.reduce((acc, input) => reduce(acc, input).state, state);
}

describe('cursor movement', () => {
  it('moves down through a column', () => {
    const state = run(createState(categories(), 1), 'down');
    expect(currentItem(state)?.id).toBe('b');
  });

  it('does not wrap past the last item', () => {
    // The real XMB stops dead at the end of a column.
    const state = run(createState(categories(), 1), 'down', 'down', 'down', 'down');
    expect(currentItem(state)?.id).toBe('c');
  });

  it('does not wrap above the first item', () => {
    const state = run(createState(categories(), 1), 'up', 'up');
    expect(currentItem(state)?.id).toBe('a');
  });

  it('reports that a blocked move did not happen', () => {
    // Drives the "wall" sound, so it has to be distinguishable from a real move.
    const start = createState(categories(), 1);
    expect(reduce(start, 'up').moved).toBe(false);
    expect(reduce(start, 'down').moved).toBe(true);
  });
});

describe('category switching', () => {
  it('moves between categories', () => {
    const state = run(createState(categories(), 1), 'right');
    expect(state.categoryIndex).toBe(2);
  });

  it('stops at the ends of the category row', () => {
    expect(run(createState(categories(), 0), 'left').categoryIndex).toBe(0);
    expect(run(createState(categories(), 2), 'right').categoryIndex).toBe(2);
  });

  it('remembers each category cursor position', () => {
    // Scroll to the third game, leave, come back.
    let state = run(createState(categories(), 1), 'down', 'down');
    expect(currentItem(state)?.id).toBe('c');
    state = run(state, 'right', 'left');
    expect(currentItem(state)?.id).toBe('c');
  });

  it('starts a newly visited category at its first item', () => {
    const state = run(createState(categories(), 1), 'left');
    expect(currentItem(state)?.id).toBe('sound');
  });
});

describe('submenus', () => {
  it('opens a submenu and lands on its first child', () => {
    const state = run(createState(categories(), 0), 'down', 'confirm');
    expect(depth(state)).toBe(1);
    expect(currentItem(state)?.id).toBe('dpad');
  });

  it('backs out of a submenu to where it was opened from', () => {
    const state = run(createState(categories(), 0), 'down', 'confirm', 'back');
    expect(depth(state)).toBe(0);
    expect(currentItem(state)?.id).toBe('controls');
  });

  it('traps horizontal input while a submenu is open', () => {
    // Sideways movement inside a submenu would skip past the parent column.
    const opened = run(createState(categories(), 0), 'down', 'confirm');
    const attempted = reduce(opened, 'right');
    expect(attempted.moved).toBe(false);
    expect(attempted.state.categoryIndex).toBe(0);
    expect(depth(attempted.state)).toBe(1);
  });

  it('refuses to open a submenu with no children', () => {
    // Opening one would strand the user in a column with nothing in it.
    const state = run(createState(categories(), 0), 'down', 'down');
    const result = reduce(state, 'confirm');
    expect(depth(result.state)).toBe(0);
    expect(result.effect).toEqual({ type: 'blocked', item: expect.objectContaining({ id: 'empty-menu' }) });
  });

  it('ignores back at the root column', () => {
    const start = createState(categories(), 1);
    expect(reduce(start, 'back').moved).toBe(false);
  });

  it('does not remember a submenu cursor, reopening at the top', () => {
    let state = run(createState(categories(), 0), 'down', 'confirm', 'down');
    expect(currentItem(state)?.id).toBe('face');
    state = run(state, 'back', 'confirm');
    expect(currentItem(state)?.id).toBe('dpad');
  });
});

describe('activation', () => {
  it('emits a launch effect for a game', () => {
    const result = reduce(createState(categories(), 1), 'confirm');
    expect(result.effect).toEqual({ type: 'launch', item: expect.objectContaining({ id: 'a' }) });
  });

  it('emits an action effect for an action item', () => {
    const result = reduce(createState(categories(), 0), 'confirm');
    expect(result.effect).toEqual({ type: 'action', item: expect.objectContaining({ id: 'sound' }) });
  });

  it('blocks a disabled item instead of activating it', () => {
    const state = run(createState(categories(), 0), 'down', 'down', 'down');
    const result = reduce(state, 'confirm');
    expect(result.effect).toEqual({ type: 'blocked', item: expect.objectContaining({ id: 'locked' }) });
  });

  it('does nothing when confirming an info item', () => {
    const state = run(createState(categories(), 0), 'down', 'confirm');
    const result = reduce(state, 'confirm');
    expect(result.effect).toBeUndefined();
    expect(result.moved).toBe(false);
  });
});

describe('library refresh', () => {
  it('swaps in rescanned games', () => {
    const state = replaceCategoryItems(createState(categories(), 1), 'game', [game('x'), game('y')]);
    expect(currentColumn(state).items.map((i) => i.id)).toEqual(['x', 'y']);
  });

  it('clamps the cursor when the library shrinks', () => {
    // Deleting games out from under the cursor must not leave it out of range.
    let state = run(createState(categories(), 1), 'down', 'down');
    state = replaceCategoryItems(state, 'game', [game('only')]);
    expect(currentColumn(state).cursor).toBe(0);
    expect(currentItem(state)?.id).toBe('only');
  });

  it('leaves the cursor alone when another category is refreshed', () => {
    let state = run(createState(categories(), 1), 'down');
    state = replaceCategoryItems(state, 'settings', []);
    expect(currentItem(state)?.id).toBe('b');
  });

  it('does not disturb an open submenu', () => {
    // A background rescan while the user reads a submenu must not yank them out.
    let state = run(createState(categories(), 0), 'down', 'confirm');
    state = replaceCategoryItems(state, 'settings', [game('new')]);
    expect(depth(state)).toBe(1);
    expect(currentItem(state)?.id).toBe('dpad');
  });

  it('ignores an unknown category id', () => {
    const before = createState(categories(), 1);
    expect(replaceCategoryItems(before, 'nope', [])).toBe(before);
  });
});

describe('edge cases', () => {
  it('survives an empty category list', () => {
    const state = createState([]);
    expect(currentItem(state)).toBeUndefined();
    expect(reduce(state, 'confirm').moved).toBe(false);
    expect(reduce(state, 'right').moved).toBe(false);
  });

  it('clamps an out-of-range starting category', () => {
    expect(createState(categories(), 99).categoryIndex).toBe(2);
  });

  it('handles a category with no items', () => {
    const empty: XmbCategory[] = [{ id: 'e', label: 'Empty', glyph: 'game', items: [] }];
    const state = createState(empty);
    expect(currentItem(state)).toBeUndefined();
    expect(reduce(state, 'down').moved).toBe(false);
  });
});
