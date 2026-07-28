/**
 * The XMB's animated wave.
 *
 * The PSP's wallpaper is a slow ribbon of light that undulates across the screen
 * and brightens around the cross bar. It is the single most recognisable part of
 * the shell, and a static gradient reads as obviously wrong.
 *
 * Built from a few summed sines rather than a texture: the interference between
 * incommensurable frequencies never visibly loops, which is what makes the real
 * one feel alive. Drawn with `lighter` compositing so overlapping ribbons bloom
 * where they cross.
 */

/** PSP logical resolution. Everything is authored in these coordinates. */
export const PSP_WIDTH = 480;
export const PSP_HEIGHT = 272;

interface Ribbon {
  /** Vertical centre in PSP pixels. */
  baseY: number;
  amplitude: number;
  /** Horizontal wavelength in PSP pixels. */
  wavelength: number;
  /** Radians per second. */
  speed: number;
  phase: number;
  thickness: number;
  opacity: number;
}

const RIBBONS: Ribbon[] = [
  { baseY: 150, amplitude: 16, wavelength: 380, speed: 0.22, phase: 0.0, thickness: 46, opacity: 0.2 },
  { baseY: 138, amplitude: 22, wavelength: 250, speed: -0.17, phase: 1.7, thickness: 30, opacity: 0.26 },
  { baseY: 128, amplitude: 12, wavelength: 170, speed: 0.31, phase: 3.1, thickness: 16, opacity: 0.3 },
  { baseY: 168, amplitude: 26, wavelength: 460, speed: -0.11, phase: 4.6, thickness: 60, opacity: 0.14 },
];

export interface WaveOptions {
  /** Ribbon colour; defaults to reading `--xmb-wave` off the canvas. */
  color?: string;
  /** Set false to render a single frame and stop, e.g. for screenshots. */
  animate?: boolean;
}

export class Wave {
  private readonly canvas: HTMLCanvasElement;
  private readonly ctx: CanvasRenderingContext2D;
  private frame = 0;
  private startedAt = 0;
  private running = false;
  private color: string;

  constructor(canvas: HTMLCanvasElement, options: WaveOptions = {}) {
    const ctx = canvas.getContext('2d');
    if (!ctx) {
      throw new Error('2D canvas context unavailable');
    }
    this.canvas = canvas;
    this.ctx = ctx;
    this.color = options.color ?? this.readThemeColor();
    this.resize();
  }

  /**
   * Matches the backing store to the canvas's on-screen size so the wave stays
   * sharp when the stage is scaled up to a desktop window.
   */
  resize(): void {
    const dpr = window.devicePixelRatio || 1;
    const rect = this.canvas.getBoundingClientRect();
    // Before layout runs the rect is empty; fall back to logical size.
    const cssWidth = rect.width || PSP_WIDTH;
    const cssHeight = rect.height || PSP_HEIGHT;
    this.canvas.width = Math.round(cssWidth * dpr);
    this.canvas.height = Math.round(cssHeight * dpr);
  }

  /** Re-reads the theme colour, e.g. after the month theme changes. */
  refreshColor(): void {
    this.color = this.readThemeColor();
  }

  start(): void {
    if (this.running) {
      return;
    }
    this.running = true;
    this.startedAt = performance.now();
    const loop = (now: number) => {
      if (!this.running) {
        return;
      }
      this.draw((now - this.startedAt) / 1000);
      this.frame = requestAnimationFrame(loop);
    };
    this.frame = requestAnimationFrame(loop);
  }

  stop(): void {
    this.running = false;
    cancelAnimationFrame(this.frame);
  }

  /** Draws one frame at a fixed time — deterministic, for screenshots and tests. */
  drawStill(seconds = 0): void {
    this.draw(seconds);
  }

  private draw(seconds: number): void {
    const { ctx, canvas } = this;
    // Work in PSP coordinates regardless of the backing store's real size.
    const scaleX = canvas.width / PSP_WIDTH;
    const scaleY = canvas.height / PSP_HEIGHT;

    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.save();
    ctx.scale(scaleX, scaleY);
    ctx.globalCompositeOperation = 'lighter';

    for (const ribbon of RIBBONS) {
      this.drawRibbon(ribbon, seconds);
    }

    ctx.restore();
  }

  private drawRibbon(ribbon: Ribbon, seconds: number): void {
    const { ctx } = this;
    const t = seconds * ribbon.speed + ribbon.phase;

    // Trace the ribbon's centre line, then come back along an offset line so the
    // shape can be filled with a soft vertical gradient.
    const step = 6;
    const top: Array<[number, number]> = [];
    const bottom: Array<[number, number]> = [];

    for (let x = -step; x <= PSP_WIDTH + step; x += step) {
      const primary = Math.sin((x / ribbon.wavelength) * Math.PI * 2 + t);
      // A second, slower term stops the ribbon looking like a pure sine.
      const secondary = Math.sin((x / (ribbon.wavelength * 0.41)) * Math.PI * 2 - t * 1.3);
      const y = ribbon.baseY + primary * ribbon.amplitude + secondary * ribbon.amplitude * 0.28;
      // Taper the ends so ribbons fade out at the screen edges instead of being cut.
      const edgeFade = Math.min(1, Math.min(x + step, PSP_WIDTH - x + step) / 90);
      const thickness = ribbon.thickness * Math.max(0, edgeFade);
      top.push([x, y - thickness / 2]);
      bottom.push([x, y + thickness / 2]);
    }

    const gradient = ctx.createLinearGradient(0, ribbon.baseY - 60, 0, ribbon.baseY + 60);
    gradient.addColorStop(0, withAlpha(this.color, 0));
    gradient.addColorStop(0.5, withAlpha(this.color, ribbon.opacity));
    gradient.addColorStop(1, withAlpha(this.color, 0));

    ctx.beginPath();
    top.forEach(([x, y], i) => (i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y)));
    for (let i = bottom.length - 1; i >= 0; i--) {
      ctx.lineTo(bottom[i][0], bottom[i][1]);
    }
    ctx.closePath();
    ctx.fillStyle = gradient;
    ctx.fill();
  }

  private readThemeColor(): string {
    const value = getComputedStyle(this.canvas).getPropertyValue('--xmb-wave').trim();
    return value || '#cfe6f5';
  }
}

/**
 * Applies an alpha to a `#rgb`/`#rrggbb` colour.
 *
 * Themes are authored as hex so they can also be dropped straight into CSS;
 * canvas gradients need rgba, so convert here rather than storing both.
 */
export function withAlpha(color: string, alpha: number): string {
  const hex = color.trim().replace('#', '');
  const full =
    hex.length === 3
      ? hex
          .split('')
          .map((c) => c + c)
          .join('')
      : hex;
  if (full.length !== 6) {
    // Not a hex colour — let the browser deal with it and drop the alpha.
    return color;
  }
  const r = parseInt(full.slice(0, 2), 16);
  const g = parseInt(full.slice(2, 4), 16);
  const b = parseInt(full.slice(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}
