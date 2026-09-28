/**
 * Canvas renderer for the "library card" — a shareable summary of a Smriti
 * library.
 *
 * Two rules shape everything here:
 *
 * 1. **Counts only.** The card must never contain a photograph, a thumbnail or
 *    a face crop, and never a person's name. Everything it draws comes from the
 *    `InsightsData` counters.
 * 2. **It must look like the app.** Colours are read from the live CSS custom
 *    properties and the grid cells are resolved through the browser's own
 *    `color-mix()` so the exported image matches what the user sees, in
 *    whichever theme they are using.
 *
 * The centrepiece is a year-by-month grid covering the *whole* library, not a
 * single year's days: one row per year, one cell per month, shaded by volume.
 * It reads as the shape of a life rather than the shape of a year.
 */

import type { InsightsData } from "./api/all";

export const CARD_WIDTH = 1080;
export const CARD_HEIGHT = 1350;
/** Draw at 2x so the PNG stays crisp when a phone scales it up. */
const SCALE = 2;
const MARGIN = 56;
const MONTH_LETTERS = ["J", "F", "M", "A", "M", "J", "J", "A", "S", "O", "N", "D"];

export interface CardMonthCell {
  x: number; y: number; width: number; height: number; label: string;
}
export interface CardTheme {
  paper: string;
  ink: string;
  inkSoft: string;
  inkMuted: string;
  inkFaint: string;
  line: string;
  accent: string;
  /** Intensity ramp, faintest first. */
  heat: [string, string, string, string, string];
}

/**
 * Resolve an arbitrary CSS colour expression to a concrete colour string by
 * letting the browser compute it. Used so `color-mix(in oklab, …)` in the
 * stylesheet and the canvas agree bit for bit.
 */
function resolveColor(expression: string, fallback: string): string {
  if (typeof document === "undefined") return fallback;
  const probe = document.createElement("div");
  probe.style.color = expression;
  probe.style.display = "none";
  document.body.appendChild(probe);
  const resolved = getComputedStyle(probe).color;
  probe.remove();
  return resolved && resolved !== "rgba(0, 0, 0, 0)" ? resolved : fallback;
}

/** Blend two theme colours through the browser, for soft tints and halos. */
function tint(colour: string, background: string, percent: number): string {
  return resolveColor(`color-mix(in oklab, ${colour} ${percent}%, ${background})`, background);
}

/** Read the current theme's tokens off the document. */
export function readCardTheme(root: HTMLElement = document.documentElement): CardTheme {
  const style = getComputedStyle(root);
  const token = (name: string, fallback: string) => {
    const value = style.getPropertyValue(name).trim();
    return value || fallback;
  };
  const bg = token("--bg", "#0f1015");
  const accent = token("--accent", "#9eb3d4");
  return {
    paper: token("--bg-paper", "#161821"),
    ink: token("--ink", "#e7eaf0"),
    inkSoft: token("--ink-soft", "#aeb3bf"),
    inkMuted: token("--ink-muted", "#80858f"),
    inkFaint: token("--ink-faint", "#4d525c"),
    line: token("--line", "#2a2e38"),
    accent,
    /// Same ramp as the Insights heatmap: accent mixed into the page
    /// background at 10/30/50/75/100 per cent.
    heat: [
      tint(accent, bg, 10),
      tint(accent, bg, 30),
      tint(accent, bg, 50),
      tint(accent, bg, 75),
      accent,
    ],
  };
}

/**
 * Make sure the display face is actually available before drawing. Canvas
 * silently falls back to a default serif if the font has not loaded, which
 * would make an exported card look nothing like the app.
 */
export async function ensureCardFonts(): Promise<void> {
  if (typeof document === "undefined" || !document.fonts) return;
  try {
    await Promise.all([
      document.fonts.load('500 172px "Bricolage Grotesque Variable"'),
      document.fonts.load('400 24px "JetBrains Mono"'),
      document.fonts.ready,
    ]);
  } catch {
    // Fall back to the serif/mono stacks in the font strings below.
  }
}

const DISPLAY = (size: number, weight = 500) =>
  `${weight} ${size}px "Bricolage Grotesque Variable", "Bricolage Grotesque", "Times New Roman", serif`;
const MONO = (size: number, weight = 400) =>
  `${weight} ${size}px "JetBrains Mono", ui-monospace, "SF Mono", Menlo, Consolas, monospace`;

/** Canvas has no letter-spacing everywhere, so track manually. */
function drawTracked(
  ctx: CanvasRenderingContext2D,
  text: string,
  x: number,
  y: number,
  tracking: number,
): number {
  let cursor = x;
  for (const char of text) {
    ctx.fillText(char, cursor, y);
    cursor += ctx.measureText(char).width + tracking;
  }
  return cursor - x - tracking;
}

function heatLevel(count: number, max: number): number {
  if (count === 0) return 0;
  const pct = max > 0 ? count / max : 0;
  if (pct < 0.15) return 1;
  if (pct < 0.35) return 2;
  if (pct < 0.6) return 3;
  return 4;
}

function yearSpan(data: InsightsData): string | null {
  const from = data.date_range_start?.slice(0, 4) ?? data.available_years.at(-1)?.toString();
  const to = data.date_range_end?.slice(0, 4) ?? data.available_years.at(0)?.toString();
  if (!from || !to) return null;
  return from === to ? from : `${from} — ${to}`;
}

/**
 * Draw the whole-library grid: a row per year, a cell per month. Returns the
 * bottom edge so the caller can tell whether anything fits.
 */
function drawYearGrid(
  ctx: CanvasRenderingContext2D,
  data: InsightsData,
  theme: CardTheme,
  top: number,
  available: number,
  left: number,
  width: number,
): CardMonthCell[] {
  const cells: CardMonthCell[] = [];
  const years = [...data.available_years].sort((a, b) => a - b);
  if (years.length === 0) return cells;

  const counts = data.months_by_year ?? {};
  const max = Math.max(1, ...Object.values(counts));

  const gutter = 74;
  const gapX = 6;
  const cellWidth = Math.floor((width - gutter - gapX * 11) / 12);

  /// Rows thin out on libraries spanning many years; tighten the gap before
  /// the cells themselves get unusable.
  let gapY = 6;
  let pitch = Math.floor((available - gapY * (years.length - 1)) / years.length);
  if (pitch < 13) {
    gapY = 3;
    pitch = Math.floor((available - gapY * (years.length - 1)) / years.length);
  }
  pitch = Math.max(7, pitch);
  const cellHeight = Math.max(4, pitch - gapY);

  ctx.font = MONO(15, 400);
  ctx.fillStyle = theme.inkFaint;
  for (let month = 0; month < 12; month++) {
    const x = left + gutter + month * (cellWidth + gapX) + cellWidth / 2;
    ctx.textAlign = "center";
    ctx.fillText(MONTH_LETTERS[month], x, top - 10);
    ctx.textAlign = "left";
  }

  years.forEach((year, row) => {
    const y = top + row * pitch;
    ctx.font = MONO(17, 400);
    ctx.fillStyle = theme.inkMuted;
    ctx.fillText(String(year), left, y + cellHeight - 2);

    for (let month = 1; month <= 12; month++) {
      const key = `${year}-${String(month).padStart(2, "0")}`;
      const count = counts[key] ?? 0;
      const x = left + gutter + (month - 1) * (cellWidth + gapX);
      const monthName = new Intl.DateTimeFormat("en", { month: "long" }).format(new Date(2000, month - 1, 1));
      cells.push({ x, y, width: cellWidth, height: cellHeight,
        label: `${monthName} ${year} — ${count.toLocaleString()} ${count === 1 ? "photo" : "photos"}` });
      ctx.fillStyle = theme.heat[heatLevel(count, max)];
      ctx.beginPath();
      ctx.roundRect(x, y, cellWidth, cellHeight, Math.min(3, cellHeight / 4));
      ctx.fill();
    }
  });
  return cells;
}

/** Fallback for a backend that predates `months_by_year`: one year, by day. */
function drawYearDays(
  ctx: CanvasRenderingContext2D,
  data: InsightsData,
  theme: CardTheme,
  top: number,
  available: number,
  left: number,
  width: number,
): void {
  const year = data.heatmap_year;
  const start = new Date(year, 0, 1);
  start.setDate(start.getDate() - start.getDay());
  const cells: Array<{ count: number; inYear: boolean }> = [];
  const cursor = new Date(start);
  for (let i = 0; i < 53 * 7; i++) {
    const month = String(cursor.getMonth() + 1).padStart(2, "0");
    const day = String(cursor.getDate()).padStart(2, "0");
    cells.push({
      count: data.heatmap[`${cursor.getFullYear()}-${month}-${day}`] ?? 0,
      inYear: cursor.getFullYear() === year,
    });
    cursor.setDate(cursor.getDate() + 1);
  }
  const max = Math.max(1, ...cells.map((cell) => cell.count));
  const gap = 3;
  const size = Math.floor((width - gap * 52) / 53);
  const rows = 7;
  const height = size * rows + gap * (rows - 1);
  const gridTop = top + Math.max(0, (available - height) / 2);

  cells.forEach((cell, index) => {
    const column = Math.floor(index / rows);
    const row = index % rows;
    ctx.globalAlpha = cell.inYear ? 1 : 0.35;
    ctx.fillStyle = theme.heat[heatLevel(cell.count, max)];
    ctx.beginPath();
    ctx.roundRect(left + column * (size + gap), gridTop + row * (size + gap), size, size, 2);
    ctx.fill();
  });
  ctx.globalAlpha = 1;
}

/**
 * Draw the card. The canvas is resized to the 2x backing store here so callers
 * only need to pass an element.
 */
export function renderShareCard(
  canvas: HTMLCanvasElement,
  data: InsightsData,
  theme: CardTheme,
): CardMonthCell[] {
  let cells: CardMonthCell[] = [];
  canvas.width = CARD_WIDTH * SCALE;
  canvas.height = CARD_HEIGHT * SCALE;
  const ctx = canvas.getContext("2d");
  if (!ctx) return cells;

  ctx.save();
  ctx.scale(SCALE, SCALE);
  ctx.textBaseline = "alphabetic";
  ctx.textAlign = "left";

  const innerWidth = CARD_WIDTH - MARGIN * 2;
  const right = CARD_WIDTH - MARGIN;

  // ---- paper -------------------------------------------------------------
  ctx.fillStyle = theme.paper;
  ctx.fillRect(0, 0, CARD_WIDTH, CARD_HEIGHT);
  const wash = ctx.createLinearGradient(0, 0, 0, CARD_HEIGHT);
  wash.addColorStop(0, "rgba(255,255,255,0.020)");
  wash.addColorStop(0.55, "rgba(255,255,255,0)");
  wash.addColorStop(1, "rgba(0,0,0,0.045)");
  ctx.fillStyle = wash;
  ctx.fillRect(0, 0, CARD_WIDTH, CARD_HEIGHT);

  ctx.strokeStyle = theme.line;
  ctx.lineWidth = 1;
  ctx.strokeRect(MARGIN / 2, MARGIN / 2, CARD_WIDTH - MARGIN, CARD_HEIGHT - MARGIN);

  // ---- wordmark ----------------------------------------------------------
  ctx.font = MONO(22, 500);
  ctx.fillStyle = theme.inkMuted;
  const markWidth = drawTracked(ctx, "SMRITI", MARGIN, 112, 6);
  ctx.fillStyle = theme.accent;
  ctx.beginPath();
  ctx.arc(MARGIN + markWidth + 15, 105, 4, 0, Math.PI * 2);
  ctx.fill();

  // ---- year span ---------------------------------------------------------
  const span = yearSpan(data);
  if (span) {
    ctx.font = MONO(32, 400);
    ctx.fillStyle = theme.inkMuted;
    drawTracked(ctx, span, MARGIN, 190, 3.4);
  }

  // ---- hero count --------------------------------------------------------
  const heroText = data.total_photos.toLocaleString();
  let heroSize = 176;
  ctx.font = DISPLAY(heroSize, 500);
  while (heroSize > 84 && ctx.measureText(heroText).width > innerWidth) {
    heroSize -= 6;
    ctx.font = DISPLAY(heroSize, 500);
  }

  /// A soft halo behind the number so it sits in light rather than on paper.
  /// The outer stop is the paper colour, not `transparent`, which would fringe.
  const haloY = 350;
  const haloRadius = 340;
  const halo = ctx.createRadialGradient(MARGIN + 150, haloY - 60, 20, MARGIN + 150, haloY - 60, haloRadius);
  halo.addColorStop(0, tint(theme.accent, theme.paper, 17));
  halo.addColorStop(1, theme.paper);
  ctx.fillStyle = halo;
  ctx.beginPath();
  ctx.arc(MARGIN + 150, haloY - 60, haloRadius, 0, Math.PI * 2);
  ctx.fill();

  ctx.font = DISPLAY(heroSize, 500);
  ctx.fillStyle = theme.ink;
  ctx.fillText(heroText, MARGIN, 396);

  ctx.font = MONO(26, 500);
  ctx.fillStyle = theme.inkMuted;
  drawTracked(ctx, data.total_photos === 1 ? "PHOTOGRAPH" : "PHOTOGRAPHS", MARGIN, 444, 5);

  /// The one spot of colour on the card.
  ctx.fillStyle = theme.accent;
  ctx.beginPath();
  ctx.roundRect(MARGIN, 464, 64, 4, 2);
  ctx.fill();

  // ---- rule --------------------------------------------------------------
  ctx.strokeStyle = theme.line;
  ctx.beginPath();
  ctx.moveTo(MARGIN, 516.5);
  ctx.lineTo(right, 516.5);
  ctx.stroke();

  // ---- counters ----------------------------------------------------------
  const counters = (
    [
      [data.people_count, "people"],
      [data.album_count, "albums"],
      [data.city_count, "cities"],
      [data.country_count, "countries"],
    ] as Array<[number, string]>
  ).filter(([value]) => value > 0);

  let nextTop = 570;
  if (counters.length > 0) {
    const columnWidth = innerWidth / counters.length;
    counters.forEach(([value, label], index) => {
      const x = MARGIN + columnWidth * index;
      ctx.font = DISPLAY(52, 500);
      ctx.fillStyle = theme.ink;
      ctx.fillText(value.toLocaleString(), x, 570);
      ctx.font = MONO(20, 400);
      ctx.fillStyle = theme.inkMuted;
      drawTracked(ctx, label.toUpperCase(), x, 604, 3.2);
    });
    nextTop = 664;
  }

  // ---- the whole library, year by year -----------------------------------
  const counts = data.months_by_year ?? {};
  const hasMonths = Object.keys(counts).length > 0;
  const footerTop = CARD_HEIGHT - MARGIN - 74;
  const gridTop = nextTop + 34;
  const gridHeight = footerTop - 96 - gridTop;

  ctx.font = MONO(19, 400);
  ctx.fillStyle = theme.inkMuted;
  drawTracked(ctx, hasMonths ? "EVERY YEAR, EVERY MONTH" : `${data.heatmap_year}, DAY BY DAY`, MARGIN, nextTop + 12, 3);

  if (gridHeight > 24) {
    if (hasMonths) {
      cells = drawYearGrid(ctx, data, theme, gridTop, gridHeight, MARGIN, innerWidth);
    } else {
      drawYearDays(ctx, data, theme, gridTop, gridHeight, MARGIN, innerWidth);
    }
  }

  // ---- footer ------------------------------------------------------------
  ctx.font = MONO(22, 400);
  ctx.fillStyle = theme.inkSoft;
  drawTracked(ctx, "0 SERVERS · 0 SUBSCRIPTIONS · ALL ON MY DRIVE", MARGIN, footerTop, 2.4);

  ctx.font = MONO(20, 400);
  ctx.fillStyle = theme.inkFaint;
  drawTracked(ctx, "chivukulavirinchi.github.io/photovault", MARGIN, footerTop + 36, 1.6);

  ctx.restore();
  return cells;
}
