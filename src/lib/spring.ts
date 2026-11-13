/** Пружина для «жидких» анимаций. Слегка недодемпфирована — даёт характерное колыхание. */
export class Spring {
  v = 0;
  constructor(
    public x: number,
    public k = 240,
    public c = 24,
    public target = x,
  ) {}

  step(dt: number) {
    const f = -this.k * (this.x - this.target) - this.c * this.v;
    this.v += f * dt;
    this.x += this.v * dt;
  }

  get resting() {
    return Math.abs(this.v) < 0.02 && Math.abs(this.x - this.target) < 0.05;
  }

  snap() {
    this.x = this.target;
    this.v = 0;
  }
}
