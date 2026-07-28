/**
 * Navigation sounds, synthesised.
 *
 * The XMB's clicks and chimes are Sony recordings, so they cannot ship here.
 * These are built from oscillators instead — short, soft, and pitched to sit in
 * the same register, so the shell feels responsive rather than silent.
 *
 * The context is created lazily on the first cue: browsers refuse to start audio
 * before a user gesture, and constructing one at load time just yields a
 * suspended context that never produces sound.
 */

export type Cue = 'move' | 'confirm' | 'back' | 'blocked' | 'boot';

export class XmbAudio {
  private context: AudioContext | null = null;
  private master: GainNode | null = null;
  private enabled = true;

  setEnabled(enabled: boolean): void {
    this.enabled = enabled;
    if (this.master) {
      this.master.gain.value = enabled ? 0.25 : 0;
    }
  }

  isEnabled(): boolean {
    return this.enabled;
  }

  play(cue: Cue): void {
    if (!this.enabled) {
      return;
    }
    const ctx = this.ensureContext();
    if (!ctx) {
      return;
    }
    // A context created before the first gesture starts suspended.
    if (ctx.state === 'suspended') {
      void ctx.resume();
    }

    switch (cue) {
      case 'move':
        // Tight, quiet blip — this fires on every cursor step.
        this.blip({ frequency: 1180, duration: 0.05, gain: 0.5, type: 'sine' });
        break;
      case 'confirm':
        this.blip({ frequency: 880, duration: 0.07, gain: 0.6, type: 'sine' });
        this.blip({ frequency: 1320, duration: 0.11, gain: 0.5, type: 'sine', delay: 0.05 });
        break;
      case 'back':
        this.blip({ frequency: 760, duration: 0.09, gain: 0.5, type: 'sine' });
        this.blip({ frequency: 480, duration: 0.12, gain: 0.4, type: 'sine', delay: 0.05 });
        break;
      case 'blocked':
        this.blip({ frequency: 200, duration: 0.14, gain: 0.5, type: 'triangle' });
        break;
      case 'boot':
        // A spread major chord, the way a console announces itself.
        [392, 523.25, 659.25, 783.99].forEach((frequency, i) => {
          this.blip({
            frequency,
            duration: 1.5 - i * 0.15,
            gain: 0.32,
            type: 'sine',
            delay: i * 0.09,
            attack: 0.06,
          });
        });
        break;
    }
  }

  private blip(options: {
    frequency: number;
    duration: number;
    gain: number;
    type: OscillatorType;
    delay?: number;
    attack?: number;
  }): void {
    const ctx = this.context;
    const master = this.master;
    if (!ctx || !master) {
      return;
    }

    const start = ctx.currentTime + (options.delay ?? 0);
    const attack = options.attack ?? 0.005;

    const osc = ctx.createOscillator();
    osc.type = options.type;
    osc.frequency.setValueAtTime(options.frequency, start);

    const envelope = ctx.createGain();
    // Ramp rather than step, or every cue starts with a click.
    envelope.gain.setValueAtTime(0, start);
    envelope.gain.linearRampToValueAtTime(options.gain, start + attack);
    // Exponential decay cannot reach exactly zero, hence the small floor.
    envelope.gain.exponentialRampToValueAtTime(0.0001, start + options.duration);

    osc.connect(envelope).connect(master);
    osc.start(start);
    osc.stop(start + options.duration + 0.02);
  }

  private ensureContext(): AudioContext | null {
    if (this.context) {
      return this.context;
    }
    // Older Safari only exposes the prefixed constructor.
    const Ctor =
      window.AudioContext ??
      (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!Ctor) {
      return null;
    }
    this.context = new Ctor();
    this.master = this.context.createGain();
    this.master.gain.value = this.enabled ? 0.25 : 0;
    this.master.connect(this.context.destination);
    return this.context;
  }
}
