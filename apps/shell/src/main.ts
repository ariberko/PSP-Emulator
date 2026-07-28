/**
 * Boots the XMB shell.
 *
 * Owns the pieces the pure modules deliberately avoid: the stage's scale, the
 * category tree built from the host's library, and the wiring from input through
 * the state machine to the renderer and the effects the host must run.
 */

import './style.css';

import { createBridge } from './platform/bridge';
import {
  gameSublabel,
  type Game,
  type HostBridge,
  type MediaItem,
  type MediaScan,
  type PadProfile,
  type Settings,
} from './platform/types';
import { mediaSublabel, MediaSurface } from './xmb/media';
import { XmbAudio } from './xmb/audio';
import { keyBindings, InputRouter, type ControllerChange } from './xmb/input';
import { faceGlyphs, type ControllerInfo } from './xmb/pad';
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
  private readonly media: MediaSurface;
  private readonly stage: HTMLElement;
  private state: XmbState;
  private wave: Wave | null = null;
  private input: InputRouter | null = null;
  private settings: Settings | null = null;
  private controllers: ControllerInfo[] = [];
  private padProfile: PadProfile | null = null;
  private toastTimer = 0;

  constructor(root: HTMLElement) {
    this.bridge = createBridge();
    this.stage = root;
    this.view = new XmbView(root);
    this.media = new MediaSurface(root, {
      onError: (text) => this.toast(text),
    });
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
    await this.loadPadProfile();
    await this.refreshLibrary({ announce: false });
    await this.refreshMedia({ announce: false });
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
    this.input = new InputRouter(
      (action) => {
        // A photo or video surface owns input while it is up, so the cursor does
        // not move behind it.
        if (this.media.isOpen) {
          this.handleMediaInput(action);
          return;
        }

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
      },
      { onControllerChange: (change) => this.onControllerChange(change) },
    );
    this.input.attach(window);
    // A pad that was already paired before launch fires no connect event, so ask
    // once at startup to get its glyphs and name right.
    this.syncControllers();
  }

  /**
   * Input while a photo or video fills the screen.
   *
   * Only ○ and ✕ do anything: directions are swallowed rather than moving a
   * cursor the user cannot see.
   */
  private handleMediaInput(action: 'up' | 'down' | 'left' | 'right' | 'confirm' | 'back'): void {
    if (action === 'back') {
      this.audio.play('back');
      this.media.closeOverlay();
      return;
    }
    if (action === 'confirm') {
      const state = this.media.togglePlayback();
      // A photo has nothing to toggle, so ✕ leaving it open is correct.
      if (state !== 'idle') {
        this.audio.play('confirm');
      }
    }
  }

  /**
   * Reacts to a controller being paired or removed.
   *
   * Pairing a pad over Bluetooth gives no feedback that the app noticed, so this
   * says so explicitly and switches the on-screen hints to that pad's own button
   * labels — ✕/○ for PlayStation, A/B for Xbox.
   */
  private onControllerChange(change: ControllerChange): void {
    this.syncControllers();
    this.audio.play(change.type === 'connected' ? 'confirm' : 'back');
    this.toast(
      change.type === 'connected'
        ? `${change.controller.name} connected`
        : `${change.controller.name} disconnected`,
    );
    // Keep the Settings entry's reading current.
    this.refreshSettingsColumn();
  }

  private syncControllers(): void {
    const controllers = this.input?.connectedControllers() ?? [];
    this.controllers = controllers;
    // The first pad wins the glyphs; mixing labels across pads would be worse
    // than picking one.
    this.view.setFaceGlyphs(faceGlyphs(controllers[0]?.faceStyle ?? 'generic'));
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

  /**
   * Adopts the controller mapping PPSSPP already has.
   *
   * If someone has remapped ✕ in PPSSPP, the XMB agreeing with the game beats the
   * two disagreeing. Failure is silent by design: the built-in mapping already
   * works, so a missing or unreadable controls.ini is not worth a warning.
   */
  private async loadPadProfile(): Promise<void> {
    try {
      this.padProfile = await this.bridge.padProfile();
      this.input?.setPadOverrides(this.padProfile.buttons);
    } catch {
      this.padProfile = null;
    }
    // The Settings column was built before this resolved, so its Controller
    // reading is stale until rebuilt.
    this.refreshSettingsColumn();
  }

  /** Rebuilds the Settings items so their readings reflect current state. */
  private refreshSettingsColumn(): void {
    this.state = replaceCategoryItems(this.state, 'settings', this.settingsItems());
    this.view.render(this.state);
  }

  /** Rescans media folders and refills the Photo, Music and Video categories. */
  private async refreshMedia(options: { announce: boolean }): Promise<void> {
    let scan: MediaScan;
    try {
      scan = await this.bridge.scanMedia();
    } catch (error) {
      this.toast(`Media scan failed: ${message(error)}`);
      return;
    }

    this.state = replaceCategoryItems(this.state, 'photo', this.mediaItems(scan.photos, 'photo'));
    this.state = replaceCategoryItems(this.state, 'music', this.mediaItems(scan.music, 'music'));
    this.state = replaceCategoryItems(this.state, 'video', this.mediaItems(scan.videos, 'video'));
    this.view.render(this.state);

    if (scan.missing_roots.length > 0) {
      this.toast(`Media folder not found: ${scan.missing_roots[0]}`);
    } else if (options.announce) {
      const total = scan.photos.length + scan.music.length + scan.videos.length;
      this.toast(
        total === 0
          ? 'No media found in those folders'
          : `${total} media file${total === 1 ? '' : 's'} found`,
      );
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
      { id: 'photo', label: 'Photo', glyph: 'photo', items: this.mediaItems([], 'photo') },
      { id: 'music', label: 'Music', glyph: 'music', items: this.mediaItems([], 'music') },
      { id: 'video', label: 'Video', glyph: 'video', items: this.mediaItems([], 'video') },
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

  /**
   * Items for one media category.
   *
   * The empty state names the category so the hint is actionable rather than a
   * generic "nothing here".
   */
  private mediaItems(items: MediaItem[], kind: 'photo' | 'music' | 'video'): XmbItem[] {
    const glyph = kind === 'photo' ? 'photo' : kind === 'music' ? 'music' : 'video';
    const list: XmbItem[] = items.map((item) => ({
      id: item.path,
      label: item.title,
      sublabel: mediaSublabel(item),
      kind: 'action',
      icon: glyph,
      payload: item,
    }));

    if (list.length === 0) {
      const noun = kind === 'photo' ? 'photos' : kind === 'music' ? 'music' : 'videos';
      list.push({
        id: `no-${kind}`,
        label: `No ${noun} found`,
        sublabel: 'Add a media folder in Settings',
        kind: 'info',
        icon: glyph,
      });
    }

    list.push({
      id: 'add-media-folder',
      label: 'Add Media Folder',
      sublabel: 'Choose where your photos, music and video live',
      kind: 'action',
      icon: 'folder',
    });

    return list;
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
      // Sits next to Emulator: both are about the hardware this shell talks to,
      // and it is the first thing to check when a pad is not responding.
      {
        id: 'controller',
        label: 'Controller',
        sublabel: this.controllerSummary(),
        kind: 'action',
        icon: 'controller',
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
        children: this.controlBindingItems(),
      },
    ];
  }

  /**
   * Reading for the Settings entry: which pads are visible, and whether PPSSPP's
   * own mapping was picked up.
   */
  private controllerSummary(): string {
    const imported = this.padProfile?.buttons ?? {};
    const bindings = Object.values(imported).reduce((sum, list) => sum + list.length, 0);
    const suffix = bindings > 0 ? '  ·  using PPSSPP’s mapping' : '';

    if (this.controllers.length === 0) {
      // Kept short when there is a suffix to fit alongside it; the full "connect
      // one by USB or Bluetooth" hint is only useful when there is nothing else
      // to report.
      return suffix
        ? `No controller yet${suffix}`
        : 'No controller detected — connect one by USB or Bluetooth';
    }

    const name =
      this.controllers.length === 1
        ? this.controllers[0].name
        : `${this.controllers[0].name} +${this.controllers.length - 1} more`;
    return `${name}${suffix}`;
  }

  /**
   * Control list, labelled for whatever is connected.
   *
   * Showing "✕" to someone holding an Xbox pad is a small thing that reads as
   * carelessness, so the pad's own labels are used when one is present.
   */
  private controlBindingItems(): XmbItem[] {
    const glyphs = faceGlyphs(this.controllers[0]?.faceStyle ?? 'generic');
    const padColumn = [
      'D-pad / left stick',
      'D-pad / left stick',
      glyphs.confirm,
      glyphs.back,
    ];

    return keyBindings().map((binding, index) => ({
      id: `binding-${index}`,
      label: binding.action,
      sublabel:
        this.controllers.length > 0
          ? `${binding.keys.join(' / ')}   ·   ${padColumn[index]}`
          : binding.keys.join('   /   '),
      kind: 'info',
      icon: 'info',
    }));
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
      // A short buzz as the handoff to PPSSPP begins, the way a console
      // acknowledges a launch.
      this.input?.rumbleAll({ duration: 140, strong: 0.45, weak: 0.25 });
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

    // Media items are identified by their payload rather than a fixed id, since
    // their ids are file paths.
    const media = effect.item.payload as MediaItem | undefined;
    if (media && effect.item.id !== 'add-media-folder') {
      this.openMedia(media);
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
        this.refreshSettingsColumn();
        this.toast(`Sound effects ${enabled ? 'on' : 'off'}`);
        break;
      }

      case 'add-media-folder': {
        const updated = await this.bridge.addMediaFolder();
        if (!updated) {
          this.toast(
            this.bridge.kind === 'browser'
              ? 'Folder picking needs the desktop app'
              : 'No folder chosen',
          );
          break;
        }
        this.settings = updated;
        await this.refreshMedia({ announce: true });
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

      case 'controller': {
        this.syncControllers();
        // Re-read in case PPSSPP was configured since launch. This also rebuilds
        // the Settings column, so the reading below is current either way.
        await this.loadPadProfile();
        const source = this.padProfile?.source;

        if (this.controllers.length === 0) {
          this.toast(
            source
              ? `No controller detected, but PPSSPP’s mapping was read from ${source}`
              : 'No controller detected. Pair it, then press a button — some pads stay idle until then.',
          );
          break;
        }
        if (source) {
          this.toast(`Using PPSSPP’s controller mapping from ${source}`);
        }
        // Buzzing the pad is the clearest possible confirmation that the right
        // device is connected and that output reaches it too.
        this.input?.rumbleAll({ duration: 220, strong: 0.5, weak: 0.3 });
        if (!source) {
          this.toast(`${this.controllers.map((c) => c.name).join(', ')} — rumble sent`);
        }
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

  /** Opens a photo, or starts a track or video. */
  private openMedia(item: MediaItem): void {
    const url = this.bridge.mediaUrl(item);
    if (!url) {
      this.toast(
        this.bridge.kind === 'browser'
          ? 'Playing local files needs the desktop app'
          : `Could not open ${item.title}`,
      );
      return;
    }

    switch (item.kind) {
      case 'photo':
        this.media.showPhoto(item, url);
        break;
      case 'video':
        // Stop music first: two soundtracks at once is never what was meant.
        this.media.stopMusic();
        this.media.playVideo(item, url);
        break;
      case 'music':
        this.media.playMusic(item, url);
        this.toast(`Playing ${item.title}`);
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
