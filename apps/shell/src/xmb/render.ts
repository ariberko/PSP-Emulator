/**
 * Renders XMB state into the DOM.
 *
 * Everything is laid out in PSP logical pixels (480×272) inside a stage that CSS
 * scales up to the window. Positioning in the console's real coordinate space is
 * what keeps proportions honest — icon sizes, bar height and spacing stay in the
 * relationship they have on hardware, at any window size.
 *
 * The layout is the "cross" the CrossMediaBar is named for: categories run along
 * a horizontal line, the selected category's items run down a vertical one, and
 * the selection sits where they meet. Items scrolling up past the bar fade out
 * under it via a CSS mask rather than being hard-clipped.
 */

import { glyphSvg, iconMarkup, isGlyphName } from './glyphs';
import {
  currentCategory,
  currentColumn,
  currentItem,
  depth,
  type XmbCategory,
  type XmbItem,
  type XmbState,
} from './model';

/** Horizontal distance between category icons, in PSP pixels. */
export const CATEGORY_SPACING = 72;
/** Vertical distance between items, in PSP pixels. */
export const ITEM_SPACING = 40;

export class XmbView {
  private readonly categoryRow: HTMLElement;
  private readonly columnStack: HTMLElement;
  private readonly backdrop: HTMLElement;
  private readonly clock: HTMLElement;
  private readonly hints: HTMLElement;
  private lastBackground: string | null = null;

  constructor(root: HTMLElement) {
    root.innerHTML = TEMPLATE;
    this.categoryRow = must(root, '.xmb-categories');
    this.columnStack = must(root, '.xmb-columns');
    this.backdrop = must(root, '.xmb-item-backdrop');
    this.clock = must(root, '.xmb-clock');
    this.hints = must(root, '.xmb-hints');
    this.startClock();
  }

  /**
   * Relabels the footer hints for the connected controller.
   *
   * Showing ✕/○ to someone holding an Xbox pad is the kind of detail that reads
   * as careless, so the labels follow whatever is plugged in.
   */
  setFaceGlyphs(glyphs: { confirm: string; back: string }): void {
    this.hints.innerHTML = `
      <span><span class="xmb-hint-key">${escapeHtml(glyphs.confirm)}</span>Enter</span>
      <span><span class="xmb-hint-key">${escapeHtml(glyphs.back)}</span>Back</span>
    `;
  }

  render(state: XmbState): void {
    this.renderCategories(state);
    this.renderColumns(state);
    this.renderBackdrop(state);
  }

  private renderCategories(state: XmbState): void {
    // The row slides so the selected category always sits at the cross.
    this.categoryRow.style.setProperty('--category-index', String(state.categoryIndex));
    // Categories dim while a submenu is open, the way focus recedes on hardware.
    this.categoryRow.classList.toggle('is-recessed', depth(state) > 0);

    this.categoryRow.innerHTML = state.categories
      .map((category, index) => this.categoryMarkup(category, index === state.categoryIndex))
      .join('');
  }

  private categoryMarkup(category: XmbCategory, selected: boolean): string {
    const glyph = isGlyphName(category.glyph) ? category.glyph : 'umd';
    return `
      <div class="xmb-category ${selected ? 'is-selected' : ''}">
        <div class="xmb-category-icon">${glyphSvg(glyph)}</div>
        <div class="xmb-category-label">${escapeHtml(category.label)}</div>
      </div>
    `;
  }

  private renderColumns(state: XmbState): void {
    // Each open submenu is its own column, offset to the right of its parent.
    this.columnStack.innerHTML = state.columns
      .map((column, level) => {
        const isActive = level === state.columns.length - 1;
        const items = column.items
          .map((item, index) => this.itemMarkup(item, index === column.cursor && isActive))
          .join('');
        return `
          <div class="xmb-column ${isActive ? 'is-active' : 'is-parent'}"
               style="--cursor:${column.cursor}; --level:${level};">
            ${items || EMPTY_COLUMN}
          </div>
        `;
      })
      .join('');
  }

  private itemMarkup(item: XmbItem, selected: boolean): string {
    const classes = [
      'xmb-item',
      selected ? 'is-selected' : '',
      item.disabled ? 'is-disabled' : '',
      item.icon?.startsWith('data:') ? 'has-cover' : '',
    ]
      .filter(Boolean)
      .join(' ');

    const fallback = item.kind === 'game' ? 'umd' : 'folder';
    const sublabel = item.sublabel
      ? `<div class="xmb-item-sublabel">${escapeHtml(item.sublabel)}</div>`
      : '';
    const chevron = item.kind === 'submenu' ? '<div class="xmb-item-chevron">›</div>' : '';

    return `
      <div class="${classes}">
        <div class="xmb-item-icon">${iconMarkup(item.icon, fallback)}</div>
        <div class="xmb-item-text">
          <div class="xmb-item-label">${escapeHtml(item.label)}</div>
          ${sublabel}
        </div>
        ${chevron}
      </div>
    `;
  }

  /**
   * Fades the selected game's PIC1 in behind the bar.
   *
   * Only touched when the image actually changes — reassigning the same URL
   * restarts the CSS transition and makes the backdrop flicker on every keypress.
   */
  private renderBackdrop(state: XmbState): void {
    const background = currentItem(state)?.background ?? null;
    if (background === this.lastBackground) {
      return;
    }
    this.lastBackground = background;
    if (background) {
      this.backdrop.style.backgroundImage = `url("${background}")`;
      this.backdrop.classList.add('is-visible');
    } else {
      this.backdrop.classList.remove('is-visible');
    }
  }

  /** Status-bar clock, matching the PSP's 24-hour HH:MM. */
  private startClock(): void {
    const tick = () => {
      const now = new Date();
      const hours = String(now.getHours()).padStart(2, '0');
      const minutes = String(now.getMinutes()).padStart(2, '0');
      this.clock.textContent = `${hours}:${minutes}`;
    };
    tick();
    window.setInterval(tick, 15_000);
  }

  /** Current category id, for callers that need to scope a library refresh. */
  categoryIdOf(state: XmbState): string | undefined {
    return currentCategory(state)?.id;
  }

  /** Number of items in the visible column, for the help footer. */
  visibleCount(state: XmbState): number {
    return currentColumn(state).items.length;
  }
}

const EMPTY_COLUMN = `
  <div class="xmb-item is-empty">
    <div class="xmb-item-text">
      <div class="xmb-item-label">Nothing here</div>
      <div class="xmb-item-sublabel">No entries to show</div>
    </div>
  </div>
`;

const TEMPLATE = `
  <canvas class="xmb-wave"></canvas>
  <div class="xmb-item-backdrop"></div>
  <div class="xmb-status">
    <span class="xmb-clock">00:00</span>
    <span class="xmb-battery" aria-label="Battery">
      <span class="xmb-battery-shell"><span class="xmb-battery-fill"></span></span>
    </span>
  </div>
  <div class="xmb-cross">
    <div class="xmb-categories"></div>
    <div class="xmb-columns"></div>
  </div>
  <div class="xmb-hints">
    <span><span class="xmb-hint-key">✕</span>Enter</span>
    <span><span class="xmb-hint-key">○</span>Back</span>
  </div>
`;

function must(root: HTMLElement, selector: string): HTMLElement {
  const element = root.querySelector<HTMLElement>(selector);
  if (!element) {
    throw new Error(`XMB template is missing ${selector}`);
  }
  return element;
}

export function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
