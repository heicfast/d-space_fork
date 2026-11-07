<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!--
  Timeline — Bölüm 9.1 mod 4/4 (dört görüntü modu), Pillar 2.

  Dosyaların `modified_unix` (StandardInformation.modification_time veya
  fs::metadata().modified()) zaman eksenine yerleştirilir. X = zaman,
  daire boyutu = aggregate_size, Y = çakışmasız Y-only force-relax.

  Ölçek seçimi: aralık < 1 yıl ise linear, aksi durumda log (eski dosyalar
  yığılmasın). modified_unix == 0 olan düğümler (sentetik root, hata,
  bilinmiyor) "?" sol-üst gettosu'na alınır.

  Hand-rolled SVG, D3 yok. Click → drill emit (klasörlerde).
-->
<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

const { locale } = useI18n();

interface TreeNode {
  id: number;
  parent: number | null;
  name: string;
  data_size: number;
  aggregate_size: number;
  is_dir: boolean;
  score: number | null;
  score_rule: string | null;
  score_reason: string | null;
  modified_unix: number;
}

interface WindowResult {
  parent_id: number;
  parent_name: string;
  parent_aggregate_size: number;
  total_children: number;
  returned: number;
  nodes: TreeNode[];
}

interface Dot {
  id: number;
  name: string;
  size: number;
  is_dir: boolean;
  modified_unix: number;
  x: number;
  y: number;
  r: number;
  fill: string;
  unknown: boolean;
}

const props = defineProps<{ data: WindowResult | null }>();
const emit = defineEmits<{ (e: "drill", node: TreeNode): void }>();

const hoveredId = ref<number | null>(null);

const WIDTH = 720;
const HEIGHT = 360;
const MARGIN_LEFT = 60; // "?" gettosu + Y-axis
const MARGIN_RIGHT = 16;
const MARGIN_TOP = 28; // X-axis ticks
const MARGIN_BOTTOM = 24;
const PLOT_X0 = MARGIN_LEFT;
const PLOT_X1 = WIDTH - MARGIN_RIGHT;
const PLOT_Y0 = MARGIN_TOP;
const PLOT_Y1 = HEIGHT - MARGIN_BOTTOM;
const PLOT_W = PLOT_X1 - PLOT_X0;
const PLOT_H = PLOT_Y1 - PLOT_Y0;
const ITER = 60;
const MIN_RADIUS = 3;
const PADDING = 1.5;

function formatBytes(b: number): string {
  if (b <= 0) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = b;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(i >= 3 ? 1 : 0)} ${units[i]}`;
}

function scoreTierColor(score: number | null): string {
  if (score === null) return "hsl(190 35% 38%)";
  if (score <= 30) return "#7f1d1d";
  if (score <= 60) return "#78350f";
  if (score <= 85) return "#14532d";
  return "#1e3a8a";
}

function formatAgo(unix: number, nowSecs: number): string {
  if (unix === 0) return "bilinmiyor";
  const delta = Math.max(0, nowSecs - unix);
  const day = 86400;
  if (delta < day) return "<1g";
  if (delta < day * 7) return `${Math.floor(delta / day)}g`;
  if (delta < day * 30) return `${Math.floor(delta / (day * 7))}h`;
  if (delta < day * 365) return `${Math.floor(delta / (day * 30))}ay`;
  return `${Math.floor(delta / (day * 365))}y`;
}

function formatDate(unix: number): string {
  if (unix === 0) return "—";
  // Gemini review — locale-aware tarih.
  return new Date(unix * 1000).toLocaleDateString(
    locale.value === "tr" ? "tr-TR" : "en-US",
  );
}

interface Layout {
  dots: Dot[];
  xMin: number;
  xMax: number;
  useLog: boolean;
  ticks: { x: number; label: string }[];
  hasUnknown: boolean;
}

const nowSecs = Math.floor(Date.now() / 1000);

const layout = computed<Layout | null>(() => {
  if (!props.data || props.data.nodes.length === 0) return null;
  const total = props.data.parent_aggregate_size;
  if (total <= 0) return null;

  const canvasArea = PLOT_W * PLOT_H * 0.45;
  const items = props.data.nodes
    .filter((n) => n.aggregate_size > 0)
    .map((n) => {
      const area = (n.aggregate_size / total) * canvasArea;
      const r = Math.max(MIN_RADIUS, Math.sqrt(area / Math.PI));
      return { node: n, r };
    });

  if (items.length === 0) return null;

  // mtime=0 → "?" gettosu, ayrı listede.
  const known = items.filter((it) => it.node.modified_unix > 0);
  const unknown = items.filter((it) => it.node.modified_unix === 0);

  let xMin = nowSecs;
  let xMax = nowSecs;
  if (known.length > 0) {
    xMin = Math.min(...known.map((it) => it.node.modified_unix));
    xMax = Math.max(...known.map((it) => it.node.modified_unix));
  }
  if (xMax <= xMin) xMax = xMin + 1; // sıfır aralık koruması

  const rangeDays = (xMax - xMin) / 86400;
  const useLog = rangeDays > 365; // > 1 yıl ise log
  const refNow = Math.max(xMax, nowSecs);

  function projectX(unix: number): number {
    if (useLog) {
      // log(age_in_days + 1) — yeni dosyalar sağda, eski sola
      const ageNow = Math.max(0, refNow - unix) / 86400;
      const ageMax = Math.max(1, (refNow - xMin) / 86400);
      const t = 1 - Math.log10(ageNow + 1) / Math.log10(ageMax + 1);
      return PLOT_X0 + t * PLOT_W;
    } else {
      const t = (unix - xMin) / (xMax - xMin);
      return PLOT_X0 + t * PLOT_W;
    }
  }

  // Dots dizimi
  const dots: Dot[] = [];
  // Önce unknown'ları sola yerleştir, küçük rastgele Y
  unknown.forEach((it, i) => {
    const y = PLOT_Y0 + ((i * 17) % PLOT_H);
    dots.push({
      id: it.node.id,
      name: it.node.name,
      size: it.node.aggregate_size,
      is_dir: it.node.is_dir,
      modified_unix: 0,
      x: MARGIN_LEFT - 30,
      y,
      r: it.r,
      fill: scoreTierColor(it.node.score),
      unknown: true,
    });
  });
  // Known: x'i zaman ekseninden, y plot ortasından başla (relax dağıtacak)
  known
    .sort((a, b) => b.r - a.r)
    .forEach((it, i) => {
      const x = projectX(it.node.modified_unix);
      const y =
        PLOT_Y0 + PLOT_H / 2 + Math.sin(i * 1.3) * (PLOT_H / 4);
      dots.push({
        id: it.node.id,
        name: it.node.name,
        size: it.node.aggregate_size,
        is_dir: it.node.is_dir,
        modified_unix: it.node.modified_unix,
        x,
        y,
        r: it.r,
        fill: scoreTierColor(it.node.score),
        unknown: false,
      });
    });

  // Force relax — X sabit (zaman önemli), sadece Y'de çöz.
  // Unknown'ları da relax içine alıyoruz ama X'leri MARGIN bölgesinde sabit
  // tutuyoruz; çakışırlarsa Y'de itilirler.
  for (let pass = 0; pass < ITER; pass++) {
    for (let i = 0; i < dots.length; i++) {
      for (let j = i + 1; j < dots.length; j++) {
        const a = dots[i];
        const b = dots[j];
        const dx = b.x - a.x;
        const dy = b.y - a.y;
        const dist = Math.sqrt(dx * dx + dy * dy) || 0.0001;
        const minDist = a.r + b.r + PADDING;
        if (dist < minDist) {
          const overlap = (minDist - dist) / 2;
          // Y-only itme (X zamanı koru), ama dist çok küçükse hafifçe X'e de
          const uy = dy === 0 ? (Math.random() < 0.5 ? -1 : 1) : dy / dist;
          a.y -= uy * overlap;
          b.y += uy * overlap;
        }
      }
      // plot içinde tut
      const d = dots[i];
      const minY = PLOT_Y0 + d.r;
      const maxY = PLOT_Y1 - d.r;
      if (d.y < minY) d.y = minY;
      if (d.y > maxY) d.y = maxY;
    }
  }

  // X-axis tick'ler: aralığa göre 4-6 mantıklı işaret
  const ticks: { x: number; label: string }[] = [];
  if (known.length > 0) {
    const stops = useLog
      ? [1, 7, 30, 365, Math.max(365, rangeDays | 0)]
      : evenlySpacedStops(xMin, xMax, refNow);
    for (const ageDays of stops) {
      const unix = refNow - ageDays * 86400;
      if (unix < xMin - 86400 || unix > xMax + 86400) continue;
      ticks.push({
        x: projectX(unix),
        label: ageLabel(ageDays),
      });
    }
    // En sağa "şimdi"
    ticks.push({ x: projectX(refNow), label: "şimdi" });
  }

  return {
    dots,
    xMin,
    xMax,
    useLog,
    ticks,
    hasUnknown: unknown.length > 0,
  };
});

function ageLabel(ageDays: number): string {
  if (ageDays < 1) return "<1g";
  if (ageDays < 14) return `${Math.round(ageDays)}g`;
  if (ageDays < 60) return `${Math.round(ageDays / 7)}h`;
  if (ageDays < 365 * 2) return `${Math.round(ageDays / 30)}ay`;
  return `${(ageDays / 365).toFixed(1)}y`;
}

function evenlySpacedStops(
  xMin: number,
  xMax: number,
  refNow: number,
): number[] {
  // 5 eşit aralık (gün cinsinden), refNow'a göre yaş
  const ageMaxDays = Math.max(1, (refNow - xMin) / 86400);
  const ageMinDays = Math.max(0, (refNow - xMax) / 86400);
  const span = ageMaxDays - ageMinDays;
  if (span <= 0) return [ageMaxDays];
  const stops: number[] = [];
  for (let k = 0; k < 5; k++) {
    stops.push(ageMinDays + (span * k) / 4);
  }
  return stops;
}

const hoveredDot = computed<Dot | null>(() => {
  if (hoveredId.value === null) return null;
  return layout.value?.dots.find((d) => d.id === hoveredId.value) ?? null;
});

function onClick(d: Dot) {
  if (!d.is_dir) return;
  const node = props.data?.nodes.find((n) => n.id === d.id);
  if (node) emit("drill", node);
}
</script>

<template>
  <div class="timeline-wrap">
    <svg
      v-if="layout"
      class="timeline-svg"
      :viewBox="`0 0 ${WIDTH} ${HEIGHT}`"
      preserveAspectRatio="xMidYMid meet"
    >
      <!-- Plot çerçeve -->
      <rect
        :x="PLOT_X0"
        :y="PLOT_Y0"
        :width="PLOT_W"
        :height="PLOT_H"
        fill="#0b0d10"
        stroke="#1f242c"
        stroke-width="1"
        rx="6"
      />

      <!-- X-axis tick'ler -->
      <g class="axis">
        <line
          :x1="PLOT_X0"
          :y1="PLOT_Y1"
          :x2="PLOT_X1"
          :y2="PLOT_Y1"
          stroke="#1f242c"
          stroke-width="1"
        />
        <g v-for="(t, i) in layout.ticks" :key="i">
          <line
            :x1="t.x"
            :y1="PLOT_Y0"
            :x2="t.x"
            :y2="PLOT_Y1"
            stroke="#1f242c"
            stroke-width="0.7"
            stroke-dasharray="2 4"
          />
          <text
            :x="t.x"
            :y="PLOT_Y1 + 14"
            text-anchor="middle"
            class="axis-label"
          >
            {{ t.label }}
          </text>
        </g>
      </g>

      <!-- "?" gettosu -->
      <g v-if="layout.hasUnknown">
        <rect
          :x="2"
          :y="PLOT_Y0"
          :width="MARGIN_LEFT - 8"
          :height="PLOT_H"
          fill="#1a1f26"
          stroke="#1f242c"
          stroke-width="1"
          stroke-dasharray="3 3"
          rx="4"
        />
        <text
          :x="MARGIN_LEFT / 2 - 4"
          :y="PLOT_Y0 - 8"
          text-anchor="middle"
          class="axis-label"
        >
          ?
        </text>
      </g>

      <!-- Dots -->
      <g
        v-for="d in layout.dots"
        :key="d.id"
        :class="{ dot: true, 'dot-dir': d.is_dir, 'dot-active': hoveredId === d.id }"
        @mouseenter="hoveredId = d.id"
        @mouseleave="hoveredId = null"
        @click="onClick(d)"
      >
        <circle
          :cx="d.x"
          :cy="d.y"
          :r="d.r"
          :fill="d.fill"
          class="dot-circle"
        />
        <title>
          {{ d.name }} · {{ formatBytes(d.size) }} ·
          {{ d.unknown ? "mtime bilinmiyor" : formatDate(d.modified_unix) }}
        </title>
      </g>
    </svg>
    <div v-else class="timeline-empty">Timeline için yeterli veri yok.</div>

    <div v-if="hoveredDot" class="timeline-readout mono">
      <span class="readout-name">{{ hoveredDot.name }}</span>
      <span class="readout-size">{{ formatBytes(hoveredDot.size) }}</span>
      <span class="readout-time">
        {{
          hoveredDot.unknown
            ? "mtime bilinmiyor"
            : `${formatDate(hoveredDot.modified_unix)} · ${formatAgo(hoveredDot.modified_unix, nowSecs)} önce`
        }}
      </span>
    </div>
    <div v-if="layout && layout.useLog" class="timeline-hint mono">
      log ölçek · aralık &gt; 1 yıl
    </div>
  </div>
</template>

<style scoped>
.timeline-wrap {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px 0 4px;
}

.timeline-svg {
  width: 100%;
  height: auto;
  background: #0b0d10;
  border-radius: 10px;
  user-select: none;
}

.axis-label {
  fill: var(--muted);
  font-family: ui-monospace, monospace;
  font-size: 10px;
  pointer-events: none;
}

.dot-circle {
  stroke: #0b0d10;
  stroke-width: 1.2;
  transition: opacity 0.15s, filter 0.15s;
  opacity: 0.85;
}

.dot-dir .dot-circle {
  cursor: pointer;
}

.dot-dir:hover .dot-circle,
.dot-active .dot-circle {
  opacity: 1;
  filter: brightness(1.3)
    drop-shadow(0 0 6px rgba(36, 200, 219, 0.45));
}

.timeline-empty {
  padding: 60px 0;
  color: var(--muted);
  font-size: 13px;
  text-align: center;
}

.timeline-readout {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 6px 10px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg);
  font-size: 12px;
  flex-wrap: wrap;
}

.readout-name {
  color: var(--fg);
  font-weight: 500;
}

.readout-size {
  color: #6ee7b7;
}

.readout-time {
  color: #fcd34d;
  font-size: 11px;
}

.timeline-hint {
  font-size: 10px;
  color: var(--muted);
  text-align: right;
  letter-spacing: 0.04em;
}
</style>
