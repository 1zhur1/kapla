/** Насыщенный доминантный цвет обложки — для подсветки капли. */
export async function dominantColor(src: string): Promise<[number, number, number] | null> {
  const img = new Image();
  img.src = src;
  try {
    await img.decode();
  } catch {
    return null;
  }
  const s = 24;
  const c = document.createElement("canvas");
  c.width = c.height = s;
  const ctx = c.getContext("2d", { willReadFrequently: true });
  if (!ctx) return null;
  ctx.drawImage(img, 0, 0, s, s);
  const d = ctx.getImageData(0, 0, s, s).data;
  let best: [number, number, number] | null = null;
  let bestScore = -1;
  // Грубая квантизация по оттенку, выбираем самый «живой» кластер.
  const buckets = new Map<number, { r: number; g: number; b: number; n: number; sat: number }>();
  for (let i = 0; i < d.length; i += 4) {
    const r = d[i], g = d[i + 1], b = d[i + 2];
    const max = Math.max(r, g, b), min = Math.min(r, g, b);
    const l = (max + min) / 510;
    const sat = max === 0 ? 0 : (max - min) / max;
    if (l < 0.12 || l > 0.95) continue;
    const key = ((r >> 5) << 6) | ((g >> 5) << 3) | (b >> 5);
    const bk = buckets.get(key) ?? { r: 0, g: 0, b: 0, n: 0, sat: 0 };
    bk.r += r; bk.g += g; bk.b += b; bk.n++; bk.sat += sat;
    buckets.set(key, bk);
  }
  for (const bk of buckets.values()) {
    const score = bk.n * (0.25 + bk.sat / bk.n) ** 2;
    if (score > bestScore) {
      bestScore = score;
      best = [bk.r / bk.n, bk.g / bk.n, bk.b / bk.n];
    }
  }
  if (!best) return null;
  // Поднимаем яркость, чтобы цвет светился на чёрном.
  const m = Math.max(...best);
  const k = m < 170 ? 170 / Math.max(m, 1) : 1;
  return best.map((v) => Math.min(255, Math.round(v * k))) as [number, number, number];
}
