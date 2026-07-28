/**
 * Boots the XMB shell.
 *
 * Owns the pieces the pure modules deliberately avoid: the stage's scale, the
 * category tree built from the host's library, and the wiring from input through
 * the state machine to the renderer and the effects the host must run.
 */

import './style.css';

import { createBridge } from './platform/bridge';
import { gameSublabel, type Game, type HostBridge, type Settings } from './platform/types';
import { XmbAudio } from './xmb/audio';
import { keyBindings, InputRouter } from './xmb/input';
import {
  createState,
  reduce,
  replaceCategoryItems,
  type XmbCategory,
  type XmbEffect,
  type XmbItem,
  type XmbState,
} from './xmb/model';
import { XmbView } from './xmb/render';
import { applyTheme, themeByName, themeForDate } from './xmb/theme';
import { PSP_HEIGHT, PSP_WIDTH, Wave } from './xmb/wave';

/** The Game category starts selected, the way a PSP with a disc in it does. */
const INITIAL_CATEGORY = 4;

class Shell {
  private readonly bridge: HostBridge;
  private readonly view: XmbView;
  private readonly audio = new XmbAudio();
  private readonly stage: HTMLElement;
  private state: XmbState;
  private wave: Wave | null = null;
  private input: InputRouter | null = null;
  private settings: Settings | null = null;
  private toastTimer = 0;

  constructor(root: HTMLElement) {
    this.bridge = createBridge();
    this.stage = root;
    this.view = new XmbView(root);
    this.state = createState(this.buildCategories([]), INITIAL_CATEGORY);
  }

  async start(): Promise<void> {
    this.applyThemeForNow();
    this.startWave();
    this.bindInput();
    this.bindStageScaling();
    this.view.render(this.state);

    // Sound needs a gesture before it will play; the boot chime is attempted
    // anyway because a keypress or click often precedes load in the desktop app.
    this.audio.play('boot');

    await this.loadSettings();
    await this.refreshLibrary({ announce: false });
  }

  // --- Wiring ------------------------------------------------------------

  private startWave(): void {
    const canvas = this.stage.querySelector<HTMLCanvasElement>('.xmb-wave');
    if (!canvas) {
      return;
    }
    this.wave = new Wave(canvas);
    this.wave.start();
    window.addEventListener('resize', () => this.wave?.resize());
  }

  private bindInput(): void {
    this.input = new InputRouter((action) => {
      const { state, moved, effect } = reduce(this.state, action);
      this.state = state;

      if (moved) {
        this.audio.play(action === 'confirm' ? 'confirm' : action === 'back' ? 'back' : 'move');
      } else if (effect?.type === 'blocked') {
        this.audio.play('blocked');
      }

      this.view.render(this.state);
      if (effect) {
        void this.runEffect(effect);
      }
    });
    this.input.attach(window);
  }

  /**
   * Scales the 480×272 stage to fill the window while preserving the PSP's
   * aspect ratio.
   *
   * Done in JS rather than with viewport units so the wave canvas can be resized
   * in the same pass — its backing store has to follow the on-screen size or the
   * ribbons render soft.
   */
  private bindStageScaling(): void {
    const apply = () => {
      const scale = Math.min(
        window.innerWidth / PSP_WIDTH,
        window.innerHeight / PSP_HEIGHT,
      );
      // Leave a hair of margin so the stage never touches the window edge.
      this.stage.style.setProperty('--stage-scale', String(Math.max(1, scale * 0.98)));
      this.wave?.resize();
    };
    apply();
    window.addEventListener('resize', apply);
  }

  private applyThemeForNow(): void {
    const override = this.settings?.theme_override;
    const theme = (override ? themeByName(override) : undefined) ?? themeForDate();
    applyTheme(theme, document.documentElement);
    this.wave?.refreshColor();
  }

  // --- Data --------------------------------------------------------------

  private async loadSettings(): Promise<void> {
    try {
      this.settings = await this.bridge.getSettings();
      this.audio.setEnabled(this.settings.sound_enabled);
      this.applyThemeForNow();
    } catch (error) {
      this.toast(`Couldn't read settings: ${message(error)}`);
    }
  }

  private async refreshLibrary(options: { announce: boolean }): Promise<void> {
    try {
      const scan = await this.bridge.scanLibrary();
      this.state = replaceCategoryItems(this.state, 'game', this.gameItems(scan.games));
      this.view.render(this.state);

      if (scan.missing_roots.length > 0) {
        this.toast(`ROM folder not found: ${scan.missing_roots[0]}`);
      } else if (options.announce) {
        const count = scan.games.length;
        this.toast(count === 1 ? '1 game found' : `${count} games found`);
      }
    } catch (error) {
      this.toast(`Library scan failed: ${message(error)}`);
    }
  }

  // --- Category tree -----------------------------------------------------

  private buildCategories(games: Game[]): XmbCategory[] {
    return [
      {
        id: 'settings',
        label: 'Settings',
        glyph: 'settings',
        items: this.settingsItems(),
      },
      { id: 'photo', label: 'Photo', glyph: 'photo', items: [placeholder('No photos')] },
      { id: 'music', label: 'Music', glyph: 'music', items: [placeholder('No music')] },
      { id: 'video', label: 'Video', glyph: 'video', items: [placeholder('No videos')] },
      { id: 'game', label: 'Game', glyph: 'game', items: this.gameItems(games) },
      { id: 'network', label: 'Network', glyph: 'network', items: this.networkItems() },
    ];
  }

  private gameItems(games: Game[]): XmbItem[] {
    const items: XmbItem[] = games.map((game) => ({
      id: game.path,
      label: game.title,
      sublabel: gameSublabel(game),
      kind: 'game',
      icon: game.icon ?? 'umd',
      background: game.background ?? undefined,
      payload: game,
    }));

    if (items.length === 0) {
      items.push({
        id: 'no-games',
        label: 'No games found',
        sublabel: 'Add a ROM folder in Settings',
        kind: 'info',
        icon: 'memstick',
      });
    }

    items.push({
      id: 'refresh-library',
      label: 'Refresh Library',
      sublabel: 'Rescan your ROM folders',
      kind: 'action',
      icon: 'refresh',
    });

    return items;
  }

  private settingsItems(): XmbItem[] {
    return [
      {
        id: 'add-rom-folder',
        label: 'Add ROM Folder',
        sublabel: 'Choose where your games live',
        kind: 'action',
        icon: 'folder',
      },
      {
        id: 'toggle-sound',
        label: 'Sound Effects',
        sublabel: this.audio.isEnabled() ? 'On' : 'Off',
        kind: 'action',
        icon: 'music',
      },
      {
        id: 'emulator-status',
        label: 'Emulator',
        sublabel: 'Check the PPSSPP installation',
        kind: 'action',
        icon: 'play',
      },
      {
        id: 'system-info',
        label: 'System Information',
        sublabel: 'Version and host details',
        kind: 'action',
        icon: 'info',
      },
      {
        id: 'controls',
        label: 'Controls',
        kind: 'submenu',
        icon: 'info',
        children: keyBindings().map((binding, index) => ({
          id: `binding-${index}`,
          label: binding.action,
          sublabel: binding.keys.join('   /   '),
          kind: 'info',
          icon: 'info',
        })),
      },
    ];
  }

  private networkItems(): XmbItem[] {
    return [
      {
        id: 'cloud-saves',
        label: 'Cloud Saves',
        sublabel: 'Sync save states with your Base44 account',
        kind: 'action',
        icon: 'cloud',
      },
      {
        id: 'check-updates',
        label: 'Check for Updates',
        sublabel: 'Ask Base44 for the latest release',
        kind: 'action',
        icon: 'refresh',
      },
    ];
  }

  // --- Effects -----------------------------------------------------------

  private async runEffect(effect: XmbEffect): Promise<void> {
    if (effect.type === 'launch') {
      const game = effect.item.payload as Game | undefined;
      if (!game) {
        return;
      }
      this.toast(`Starting ${game.title}…`);
      try {
        await this.bridge.launchGame(game);
      } catch (error) {
        this.audio.play('blocked');
        this.toast(message(error));
      }
      return;
    }

    if (effect.type !== 'action') {
      return;
    }

    switch (effect.item.id) {
      case 'refresh-library':
        this.toast('Scanning…');
        await this.refreshLibrary({ announce: true });
        break;

      case 'toggle-sound': {
        const enabled = !this.audio.isEnabled();
        this.audio.setEnabled(enabled);
        this.settings = await this.bridge.saveSettings({ sound_enabled: enabled });
        // Rebuild so the item's sublabel reflects the new value.
        this.state = replaceCategoryItems(this.state, 'settings', this.settingsItems());
        this.view.render(this.state);
        this.toast(`Sound effects ${enabled ? 'on' : 'off'}`);
        break;
      }

      case 'add-rom-folder': {
        const updated = await this.bridge.addRomFolder();
        if (!updated) {
          this.toast(
            this.bridge.kind === 'browser'
              ? 'Folder picking needs the desktop app'
              : 'No folder chosen',
          );
          break;
        }
        this.settings = updated;
        await this.refreshLibrary({ announce: true });
        break;
      }

      case 'emulator-status': {
        const status = await this.bridge.emulatorStatus();
        this.toast(
          status.found
            ? `PPSSPP found at ${status.path} (${status.source})`
            : 'PPSSPP not found — set its path in Settings',
        );
        break;
      }

      case 'system-info': {
        const version = await this.bridge.hostVersion();
        this.toast(`PSP-Emulator shell · host ${version} · ${themeForDate().name} theme`);
        break;
      }

      case 'cloud-saves':
      case 'check-updates':
        // Wired to the Base44 backend; see base44/functions.
        this.toast('Sign in to Base44 to use this');
        break;
    }
  }

  private toast(text: string): void {
    let toast = this.stage.querySelector<HTMLElement>('.xmb-toast');
    if (!toast) {
      toast = document.createElement('div');
      toast.className = 'xmb-toast';
      this.stage.appendChild(toast);
    }
    toast.textContent = text;
    // Force a reflow so re-showing an already-visible toast restarts the fade.
    toast.classList.remove('is-visible');
    void toast.offsetWidth;
    toast.classList.add('is-visible');

    window.clearTimeout(this.toastTimer);
    this.toastTimer = window.setTimeout(() => toast?.classList.remove('is-visible'), 3200);
  }
}

function placeholder(label: string): XmbItem {
  return { id: label, label, kind: 'info', icon: 'folder' };
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function mount(): void {
  const stage = document.querySelector<HTMLElement>('.stage');
  if (!stage) {
    throw new Error('stage element missing from index.html');
  }
  const shell = new Shell(stage);
  void shell.start();

  // Remove the boot overlay once its animation has played out.
  const boot = document.querySelector<HTMLElement>('.boot');
  if (boot) {
    boot.addEventListener('animationend', () => boot.remove(), { once: true });
  }
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', mount, { once: true });
} else {
  mount();
}
