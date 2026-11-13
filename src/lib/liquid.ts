import { Spring } from "./spring";

export interface Shape {
  w: number;
  h: number;
  r: number;
}

interface Drop {
  x: number;
  y: number;
  v: number;
  r: number;
  life: number;
}

interface Ripple {
  x: number;
  y: number;
  r: number;
  maxR: number;
  life: number;
}

interface DirtyRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

const SHOULDER = 11;
const clamp = (v: number, a: number, b: number) => Math.min(b, Math.max(a, v));

/**
 * «Капля»: чернильная форма, свисающая с верхнего края экрана.
 * Оптимизирована под 60 FPS без shadowBlur и с грязным прямоугольником перерисовки.
 */
export class Liquid {
  w = new Spring(180, 240, 23);
  h = new Spring(34, 240, 23);
  r = new Spring(16, 240, 24);
  bumpA = new Spring(0, 150, 15);
  bumpX: Spring;
  wave = new Spring(0, 42, 12);
  glow = new Spring(0, 32, 11);

  cursor = { x: -999, y: -999, near: false };
  playing = false;
  audioLevel = 0;
  open = false;
  accent: [number, number, number] = [255, 122, 61];
  drops: Drop[] = [];
  ripples: Ripple[] = [];
  /** Деликатная вспышка световой кромки при открытии / уведомлениях. */
  flareV = 0;
  onFrame?: (w: number, h: number, r: number) => void;

  private ctx: CanvasRenderingContext2D;
  private t = 0;
  private raf = 0;
  private last = 0;
  private dpr = 1;
  private prevDirty: DirtyRect = { x: 0, y: 0, w: 0, h: 0 };
  private edgePts: [number, number][] = [];

  constructor(
    private cv: HTMLCanvasElement,
    private cx: number,
    private W: number,
    private H: number,
  ) {
    this.ctx = cv.getContext("2d")!;
    this.bumpX = new Spring(cx, 260, 26);
    this.prevDirty = { x: 0, y: 0, w: W, h: H };
    this.resize();
  }

  resize() {
    this.dpr = Math.min(2, window.devicePixelRatio || 1);
    this.cv.width = Math.round(this.W * this.dpr);
    this.cv.height = Math.round(this.H * this.dpr);
    this.cv.style.width = `${this.W}px`;
    this.cv.style.height = `${this.H}px`;
    this.prevDirty = { x: 0, y: 0, w: this.W, h: this.H };
    this.wake();
  }

  setShape(s: Shape) {
    this.w.target = s.w;
    this.h.target = s.h;
    this.r.target = s.r;
    this.wake();
  }

  setCursor(x: number, y: number, near: boolean) {
    const prevNear = this.cursor.near;
    this.cursor = { x, y, near };
    if (near || prevNear || !this.bumpA.resting) {
      this.wake();
    }
  }

  setMusic(playing: boolean, accent: [number, number, number]) {
    this.playing = playing;
    this.accent = accent;
    this.wake();
  }

  setAudioLevel(level: number) {
    this.audioLevel = clamp(level, 0, 1);
    if (this.audioLevel > 0.025) this.wake();
  }

  drop(rx?: number) {
    const x = rx ?? this.cx + (Math.random() - 0.5) * 34;
    this.drops.push({
      x,
      y: this.h.x - 4,
      v: 42,
      r: 5.5,
      life: 1,
    });
    this.ripples.push({
      x,
      y: this.open ? this.h.x * 0.45 : this.h.x * 0.5,
      r: 12,
      maxR: this.open ? Math.max(this.w.x * 0.45, 220) : 140,
      life: 1,
    });
    this.bumpA.v -= 68;
    this.wake();
  }

  flare(amount = 1) {
    this.flareV = Math.min(1, Math.max(this.flareV, amount));
    this.wake();
  }

  pulseRipple(x = this.cx, y = 22) {
    this.ripples.push({
      x,
      y,
      r: 14,
      maxR: this.open ? Math.max(this.w.x * 0.52, 280) : 160,
      life: 1,
    });
    this.wake();
  }

  wake() {
    if (!this.raf) {
      this.last = performance.now();
      this.raf = requestAnimationFrame(this.tick);
    }
  }

  private tick = (now: number) => {
    const dt = Math.min(0.033, Math.max(0.001, (now - this.last) / 1000));
    this.last = now;
    this.t += dt;

    // Бугорок тянется к курсору, когда тот рядом под каплей.
    const half = this.w.x / 2;
    const { x, y, near } = this.cursor;
    const dy = y - this.h.x;
    const dxOut = Math.max(0, Math.abs(x - this.cx) - half);
    const reach = this.open ? 88 : 126;
    if (near && dy > -6 && dy < reach && dxOut < 50) {
      const s = (1 - Math.max(0, dy) / reach) * (1 - dxOut / 50);
      this.bumpA.target = (this.open ? 6.5 : 10) * s;
      this.bumpX.target = clamp(x, this.cx - half + this.r.x + 8, this.cx + half - this.r.x - 8);
    } else {
      this.bumpA.target = 0;
    }

    const liveAudio = (this.playing || this.audioLevel > 0.025) && !this.open;
    const amp =
      this.audioLevel > 0.025
        ? (this.open ? 1.8 : 3.4) * (0.4 + this.audioLevel * 0.9)
        : this.playing && !this.open
          ? 2.2
          : 0;
    this.wave.target = amp;
    this.glow.target = this.playing || this.audioLevel > 0.025 ? clamp(0.45 + this.audioLevel * 0.65, 0, 1) : 0;

    const springs = [this.w, this.h, this.r, this.bumpA, this.bumpX, this.wave, this.glow];
    for (let i = 0; i < springs.length; i++) springs[i].step(dt);

    for (let i = 0; i < this.drops.length; i++) {
      const d = this.drops[i];
      d.v += 1100 * dt;
      d.y += d.v * dt;
      d.life -= dt * 1.9;
    }
    if (this.drops.length > 0) {
      this.drops = this.drops.filter((d) => d.life > 0);
    }

    for (let i = 0; i < this.ripples.length; i++) {
      const rp = this.ripples[i];
      rp.life -= dt * 1.35;
      const easeOut = 1 - Math.pow(Math.max(0, rp.life), 2.4);
      rp.r = 12 + (rp.maxR - 12) * easeOut;
    }
    if (this.ripples.length > 0) {
      this.ripples = this.ripples.filter((rp) => rp.life > 0);
    }

    if (this.flareV > 0) {
      this.flareV *= Math.exp(-dt * 2.2);
      if (this.flareV < 0.01) this.flareV = 0;
    }

    this.draw();
    this.onFrame?.(this.w.x, this.h.x, this.r.x);

    const busy =
      this.flareV > 0 ||
      springs.some((s) => !s.resting) ||
      liveAudio ||
      this.open ||
      this.cursor.near ||
      this.drops.length > 0 ||
      this.ripples.length > 0;
    this.raf = busy ? requestAnimationFrame(this.tick) : 0;
  };

  /** Смещение нижнего края в точке x. */
  private edge(x: number, x0: number, span: number) {
    const u = clamp((x - x0) / span, 0, 1);
    const env = Math.sin(Math.PI * u);
    const sigma = this.open ? 34 : 22;
    const bump = this.bumpA.x * Math.exp(-((x - this.bumpX.x) ** 2) / (2 * sigma * sigma));
    const t = this.t;
    const beat = 0.65 + 0.35 * Math.sin(t * 7.3) * Math.sin(t * 2.1);
    const wave =
      this.wave.x * beat * (Math.sin(x * 0.05 + t * 5.4) * 0.6 + Math.sin(x * 0.11 - t * 3.3) * 0.4);
    const jelly = -this.h.v * 0.014;
    return (bump + wave + jelly) * Math.sqrt(env);
  }

  private traceBody(ctx: CanvasRenderingContext2D, x0: number, x1: number, h: number, r: number, s: number, close = true) {
    ctx.beginPath();
    ctx.moveTo(x0 - s, 0);
    ctx.quadraticCurveTo(x0, 0, x0, s);
    ctx.lineTo(x0, h - r);
    ctx.arcTo(x0, h, x0 + r, h, r);
    for (let i = 0; i < this.edgePts.length; i++) {
      ctx.lineTo(this.edgePts[i][0], this.edgePts[i][1]);
    }
    ctx.arcTo(x1, h, x1, h - r, r);
    ctx.lineTo(x1, s);
    ctx.quadraticCurveTo(x1, 0, x1 + s, 0);
    if (close) ctx.closePath();
  }

  private draw() {
    const { ctx, dpr, cx } = this;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    const w = Math.max(40, this.w.x);
    const h = Math.max(20, this.h.x);
    const s = SHOULDER;
    const r = clamp(this.r.x, 4, Math.min(h - s, w / 2 - 2));
    const x0 = cx - w / 2;
    const x1 = cx + w / 2;
    const span = Math.max(2, w - 2 * r);
    const N = Math.max(20, Math.min(72, Math.round(span / 9)));
    const [ar, ag, ab] = this.accent;

    let maxDropY = h + 28;
    for (let i = 0; i < this.drops.length; i++) {
      maxDropY = Math.max(maxDropY, this.drops[i].y + this.drops[i].r + 12);
    }

    const curDirty: DirtyRect = {
      x: Math.max(0, Math.floor(x0 - s - 22)),
      y: 0,
      w: Math.min(this.W, Math.ceil(w + s * 2 + 44)),
      h: Math.min(this.H, Math.ceil(maxDropY + Math.abs(this.bumpA.x) + 18)),
    };

    const ux = Math.max(0, Math.min(this.prevDirty.x, curDirty.x));
    const uy = 0;
    const uw = Math.min(
      this.W - ux,
      Math.max(this.prevDirty.x + this.prevDirty.w, curDirty.x + curDirty.w) - ux,
    );
    const uh = Math.min(this.H, Math.max(this.prevDirty.h, curDirty.h));
    if (uw > 0 && uh > 0) {
      ctx.clearRect(ux, uy, uw, uh);
    }
    this.prevDirty = curDirty;

    this.edgePts.length = 0;
    for (let i = 0; i <= N; i++) {
      const px = x0 + r + (span * i) / N;
      this.edgePts.push([px, h + this.edge(px, x0 + r, span)]);
    }

    // Ореол кристальной глубины под формой
    const g = this.glow.x;
    const auraStrength = Math.max(g, this.open ? 0.56 : 0.24);
    if (auraStrength > 0.02) {
      const aura = ctx.createRadialGradient(cx, h, Math.max(10, w * 0.1), cx, h + 16, w * 0.58);
      aura.addColorStop(0, `rgba(${ar},${ag},${ab},${0.28 * auraStrength})`);
      aura.addColorStop(0.45, `rgba(110, 195, 255, ${0.12 * auraStrength})`);
      aura.addColorStop(1, "rgba(0, 0, 0, 0)");
      ctx.fillStyle = aura;
      ctx.fillRect(Math.max(0, x0 - 24), Math.max(0, h - 22), w + 48, 66);
    }

    // Глубокий дымчатый графит (Smoked Titanium Glass)
    this.traceBody(ctx, x0, x1, h, r, s);
    const fill = ctx.createLinearGradient(x0, 0, x1, h + 10);
    fill.addColorStop(0, this.open ? "rgba(15, 18, 26, 0.72)" : "rgba(13, 16, 23, 0.76)");
    fill.addColorStop(0.55, this.open ? "rgba(11, 13, 20, 0.78)" : "rgba(10, 12, 18, 0.80)");
    fill.addColorStop(1, this.open ? "rgba(8, 10, 15, 0.82)" : "rgba(8, 10, 15, 0.84)");
    ctx.fillStyle = fill;
    ctx.fill();

    // Внутренняя глубина: сдержанный акцент + холодный платиновый блик
    ctx.save();
    this.traceBody(ctx, x0, x1, h, r, s);
    ctx.clip();

    const t = this.t;
    const cursorBiasX = this.cursor.near ? clamp((this.cursor.x - cx) * 0.25, -80, 80) : 0;

    // 1) Сдержанная сфера акцента (реагирует на музыку и курсор)
    const orb1X = cx - w * 0.24 + Math.sin(t * 0.55) * (this.open ? 42 : 18) + cursorBiasX * 0.6;
    const orb1Y = h * (this.open ? 0.32 : 0.45) + Math.cos(t * 0.48) * (this.open ? 24 : 6);
    const orb1R = Math.max(w * 0.44, 130);
    const g1 = ctx.createRadialGradient(orb1X, orb1Y, 4, orb1X, orb1Y, orb1R);
    g1.addColorStop(0, `rgba(${ar}, ${ag}, ${ab}, ${this.open ? 0.16 : 0.18 + g * 0.18})`);
    g1.addColorStop(0.6, `rgba(${ar}, ${ag}, ${ab}, 0.04)`);
    g1.addColorStop(1, `rgba(${ar}, ${ag}, ${ab}, 0)`);
    ctx.fillStyle = g1;
    ctx.fillRect(x0 - s, 0, w + s * 2, h + 14);

    // 2) Холодное платиновое преломление справа
    const orb2X = cx + w * 0.26 + Math.cos(t * 0.45) * (this.open ? 46 : 20) - cursorBiasX * 0.35;
    const orb2Y = h * (this.open ? 0.65 : 0.5) + Math.sin(t * 0.6) * (this.open ? 22 : 5);
    const orb2R = Math.max(w * 0.42, 120);
    const g2 = ctx.createRadialGradient(orb2X, orb2Y, 4, orb2X, orb2Y, orb2R);
    g2.addColorStop(0, `rgba(215, 226, 248, ${this.open ? 0.1 : 0.08})`);
    g2.addColorStop(0.65, "rgba(215, 226, 248, 0.025)");
    g2.addColorStop(1, "rgba(215, 226, 248, 0)");
    ctx.fillStyle = g2;
    ctx.fillRect(x0 - s, 0, w + s * 2, h + 14);

    // Верхний глянцевый срез «линзы»
    const sheen = ctx.createLinearGradient(x0, 0, x0 + w * 0.36, h * 0.52);
    sheen.addColorStop(0, "rgba(255, 255, 255, 0.16)");
    sheen.addColorStop(0.28, "rgba(255, 255, 255, 0.045)");
    sheen.addColorStop(1, "rgba(255, 255, 255, 0)");
    ctx.fillStyle = sheen;
    ctx.fillRect(x0 - s, 0, w + s * 2, h + 12);

    // Следящий за курсором прожектор блика
    if (this.cursor.near) {
      const mx = clamp(this.cursor.x, x0, x1);
      const my = clamp(this.cursor.y, 0, h);
      const spot = ctx.createRadialGradient(mx, my, 2, mx, my, this.open ? 140 : 70);
      spot.addColorStop(0, "rgba(255, 255, 255, 0.11)");
      spot.addColorStop(0.5, `rgba(${ar},${ag},${ab},0.04)`);
      spot.addColorStop(1, "rgba(255, 255, 255, 0)");
      ctx.fillStyle = spot;
      ctx.fillRect(x0 - s, 0, w + s * 2, h + 12);
    }

    // Сдержанные кольца ряби при действиях
    for (let i = 0; i < this.ripples.length; i++) {
      const rp = this.ripples[i];
      const a = clamp(rp.life * 0.38, 0, 0.42);
      ctx.beginPath();
      ctx.arc(rp.x, rp.y, rp.r, 0, Math.PI * 2);
      ctx.strokeStyle = `rgba(255, 255, 255, ${a * 0.65})`;
      ctx.lineWidth = 1.25;
      ctx.stroke();

      ctx.beginPath();
      ctx.arc(rp.x, rp.y, rp.r * 0.72, 0, Math.PI * 2);
      ctx.strokeStyle = `rgba(${ar},${ag},${ab}, ${a * 0.55})`;
      ctx.lineWidth = 1;
      ctx.stroke();
    }
    ctx.restore();

    // Строгая титаново-стеклянная кромка с мягким акцентом
    this.traceBody(ctx, x0, x1, h, r, s);
    const prism = ctx.createLinearGradient(x0, 0, x1, h);
    prism.addColorStop(0, "rgba(255, 255, 255, 0.28)");
    prism.addColorStop(0.35, "rgba(255, 255, 255, 0.14)");
    prism.addColorStop(0.7, `rgba(${ar}, ${ag}, ${ab}, 0.22)`);
    prism.addColorStop(1, "rgba(255, 255, 255, 0.2)");
    ctx.strokeStyle = prism;
    ctx.lineWidth = 1.25;
    ctx.stroke();

    // Плавный световой луч по кромке (без радуги — платина + акцент)
    const haloA = clamp((this.open ? 0.26 : 0) + this.flareV * 0.65, 0, 0.9);
    if (haloA > 0.02 && typeof ctx.createConicGradient === "function") {
      const halo = ctx.createConicGradient(t * 0.65, cx, h * 0.5);
      halo.addColorStop(0, "rgba(255, 255, 255, 0.85)");
      halo.addColorStop(0.18, `rgba(${ar}, ${ag}, ${ab}, 0.75)`);
      halo.addColorStop(0.38, "rgba(255, 255, 255, 0.08)");
      halo.addColorStop(0.65, "rgba(255, 255, 255, 0.05)");
      halo.addColorStop(0.85, "rgba(215, 226, 248, 0.45)");
      halo.addColorStop(1, "rgba(255, 255, 255, 0.85)");
      this.traceBody(ctx, x0, x1, h, r, s, false);
      ctx.strokeStyle = halo;
      ctx.globalAlpha = haloA * 0.25;
      ctx.lineWidth = 6;
      ctx.stroke();
      ctx.globalAlpha = haloA * 0.9;
      ctx.lineWidth = 1.5;
      ctx.stroke();
      ctx.globalAlpha = 1;
    }

    // Мениск — чистый блик вдоль нижнего края
    const intensity = clamp(0.28 + this.bumpA.x * 0.045 + this.wave.x * 0.16 + g * 0.24, 0, 0.85);
    const rim = ctx.createLinearGradient(x0, 0, x1, 0);
    rim.addColorStop(0, "rgba(255, 255, 255, 0)");
    rim.addColorStop(0.28, `rgba(255, 255, 255, ${intensity * 0.42})`);
    rim.addColorStop(0.5, `rgba(255, 255, 255, ${Math.min(0.85, intensity + 0.12)})`);
    rim.addColorStop(0.74, `rgba(${ar},${ag},${ab},${intensity * 0.75})`);
    rim.addColorStop(1, `rgba(${ar},${ag},${ab},0)`);
    ctx.beginPath();
    for (let i = 0; i < this.edgePts.length; i++) {
      const [px, py] = this.edgePts[i];
      if (i === 0) ctx.moveTo(px, py - 0.9);
      else ctx.lineTo(px, py - 0.9);
    }
    ctx.strokeStyle = rim;
    ctx.lineWidth = 1.5;
    ctx.stroke();

    // Падающие хрустальные капли с «перешейком» и ярким бликом
    for (let i = 0; i < this.drops.length; i++) {
      const d = this.drops[i];
      const rr = d.r * (0.55 + 0.45 * d.life);
      ctx.globalAlpha = clamp(d.life * 1.3, 0, 1);
      if (d.y - h < 14) {
        ctx.beginPath();
        ctx.moveTo(d.x - 7, h - 1);
        ctx.quadraticCurveTo(d.x, h + 2, d.x - rr * 0.7, d.y);
        ctx.lineTo(d.x + rr * 0.7, d.y);
        ctx.quadraticCurveTo(d.x, h + 2, d.x + 7, h - 1);
        ctx.closePath();
        ctx.fillStyle = "rgba(28, 36, 56, 0.72)";
        ctx.fill();
      }
      ctx.beginPath();
      ctx.arc(d.x, d.y, rr, 0, Math.PI * 2);
      ctx.fillStyle = "rgba(34, 44, 68, 0.75)";
      ctx.fill();
      ctx.strokeStyle = "rgba(255, 255, 255, 0.75)";
      ctx.lineWidth = 1.1;
      ctx.stroke();
      ctx.beginPath();
      ctx.arc(d.x - rr * 0.32, d.y - rr * 0.32, Math.max(1.2, rr * 0.32), 0, Math.PI * 2);
      ctx.fillStyle = "rgba(255, 255, 255, 0.85)";
      ctx.fill();
      ctx.globalAlpha = 1;
    }
  }
}
