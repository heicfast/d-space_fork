<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!--
  LiveSunburst3D — canlı tarama görseli (3D katmanlı, döndürülebilir).
  ScanProgress.vue içinde 2D LiveSunburst yerine kullanılır.

  PartialNode tree → her depth seviyesi ayrı Y katmanı:
    depth 0 → merkez disk (root toplam)
    depth 1 → iç halka  (yBase=4,  height=18)
    depth 2 → dış halka (yBase=24, height=18) — katman görsel olarak
              ayrı duruyor, kullanıcı yukarıdan-yandan açıkça görür.

  Mouse drag → her yön döndürme; wheel → zoom; hover → wedge öne çıkar.

  Performance: scan sırasında her 10k entry'de partial_tree gelir;
  rebuildWedges idempotent, geometry dispose + recreate.
-->
<script setup lang="ts">
import {
  onBeforeUnmount,
  onMounted,
  ref,
  shallowRef,
  watch,
} from "vue";
import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import {
  CSS2DObject,
  CSS2DRenderer,
} from "three/addons/renderers/CSS2DRenderer.js";

interface PartialNode {
  id: number;
  parent: number | null;
  name: string;
  aggregate_size: number;
  depth: number;
  is_dir: boolean;
}

const props = defineProps<{
  partialTree: PartialNode[];
  emptyMessage?: string;
}>();

const canvasRef = ref<HTMLCanvasElement | null>(null);
const labelLayerRef = ref<HTMLDivElement | null>(null);
const hoveredLabel = ref<string>("");

/// Dilim açısı bu eşikten büyük ise label her zaman görünür. Eşik altı
/// (ince dilim) yalnız hover'da veya kamera yakın mesafede belirir.
const LABEL_ALWAYS_THRESHOLD_RAD = 0.22; // ~12.6°
const LABEL_NEAR_DISTANCE = 130; // OrbitControls min=80, max=500

interface LabelHandle {
  div: HTMLDivElement;
  mesh: THREE.Mesh;
  worldPos: THREE.Vector3;
  isSmall: boolean;
}

const PALETTE = [
  0x4e79a7, 0xf28e2c, 0xe15759, 0x76b7b2, 0x59a14f, 0xedc949, 0xaf7aa1,
  0xff9da7, 0x9c755f, 0xbab0ab, 0x5b8ff9, 0x5ad8a6,
];

const INNER_R = 28;
const RING1_INNER = 36;
const RING1_OUTER = 88;
const RING2_INNER = 95;
const RING2_OUTER = 145;
const RING_HEIGHT = 18;
const CENTER_HEIGHT = 5;
const LAYER_GAP = 2;
const RING1_Y = CENTER_HEIGHT + 1;
const RING2_Y = RING1_Y + RING_HEIGHT + LAYER_GAP;
const GAP_RAD = 0.01;

interface PulseAnim {
  mesh: THREE.Mesh;
  startTime: number;
  duration: number;
  baseY: number;
  liftAmount: number;
}

interface SceneState {
  renderer: THREE.WebGLRenderer;
  labelRenderer: CSS2DRenderer;
  scene: THREE.Scene;
  camera: THREE.PerspectiveCamera;
  controls: OrbitControls;
  center: THREE.Mesh; // Gemini review — dispose için takip
  raycaster: THREE.Raycaster;
  pointer: THREE.Vector2;
  wedges: THREE.Mesh[];
  labels: LabelHandle[];
  hovered: THREE.Mesh | null;
  pulse: PulseAnim | null;
  animation: number | null;
}
const sceneState = shallowRef<SceneState | null>(null);
const isEmpty = ref<boolean>(true);

function buildWedgeShape(
  a1: number,
  a2: number,
  innerR: number,
  outerR: number,
): THREE.Shape {
  const shape = new THREE.Shape();
  shape.absarc(0, 0, outerR, a1, a2, false);
  shape.absarc(0, 0, innerR, a2, a1, true);
  return shape;
}

function colorFor(index: number): number {
  return PALETTE[index % PALETTE.length];
}

function darken(hex: number, factor: number): number {
  const r = (hex >> 16) & 0xff;
  const g = (hex >> 8) & 0xff;
  const b = hex & 0xff;
  const rr = Math.round(r * (1 - factor));
  const gg = Math.round(g * (1 - factor));
  const bb = Math.round(b * (1 - factor));
  return (rr << 16) | (gg << 8) | bb;
}

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

function buildScene(
  canvas: HTMLCanvasElement,
  labelLayer: HTMLDivElement,
): SceneState {
  const w = canvas.clientWidth || 360;
  const h = canvas.clientHeight || 360;

  const renderer = new THREE.WebGLRenderer({
    canvas,
    antialias: true,
    alpha: true,
  });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  renderer.setSize(w, h, false);

  // CSS2D renderer — HTML div'leri 3D pozisyonda render eder; WebGL
  // overlay'in üstüne ekstra DOM layer. Crisp text, font scaling
  // tarayıcı hızlandırma ile.
  const labelRenderer = new CSS2DRenderer({ element: labelLayer });
  labelRenderer.setSize(w, h);

  const scene = new THREE.Scene();
  scene.background = null;

  const camera = new THREE.PerspectiveCamera(50, w / h, 1, 2000);
  camera.position.set(140, 160, 200);
  camera.lookAt(0, 22, 0);

  const ambient = new THREE.AmbientLight(0xffffff, 0.6);
  scene.add(ambient);
  const dir1 = new THREE.DirectionalLight(0xffffff, 0.85);
  dir1.position.set(120, 200, 80);
  scene.add(dir1);
  const dir2 = new THREE.DirectionalLight(0xffffff, 0.35);
  dir2.position.set(-80, 100, -120);
  scene.add(dir2);

  const controls = new OrbitControls(camera, canvas);
  controls.enableDamping = true;
  controls.dampingFactor = 0.1;
  controls.minDistance = 100;
  controls.maxDistance = 420;
  // Yatay eksende serbest döndürme (azimuth = 360° tam tur).
  // Dikey eğim sınırlı: kullanıcı sunburst'ü tepeden kuş bakışı
  // (~25°) ile yandan eğik (~75°) arasında görür; düz çember veya
  // tepeden direk baktığında "kaybolma" yaşamaz.
  controls.minPolarAngle = Math.PI * 0.15; // ~27° from top
  controls.maxPolarAngle = Math.PI * 0.42; // ~75° from top (15° above horizon)
  controls.target.set(0, 22, 0);
  controls.update();

  // Center disk — root toplam, koyu zemin.
  const centerGeo = new THREE.CylinderGeometry(INNER_R, INNER_R, CENTER_HEIGHT, 48);
  const centerMat = new THREE.MeshStandardMaterial({
    color: 0x2a2c34,
    roughness: 0.6,
    metalness: 0.12,
  });
  const center = new THREE.Mesh(centerGeo, centerMat);
  center.position.y = CENTER_HEIGHT / 2;
  scene.add(center);

  return {
    renderer,
    labelRenderer,
    scene,
    camera,
    controls,
    center,
    raycaster: new THREE.Raycaster(),
    pointer: new THREE.Vector2(),
    wedges: [],
    labels: [],
    hovered: null,
    pulse: null,
    animation: null,
  };
}

/// Click feedback — tıklanan wedge baseY'den +liftAmount kadar yükselir,
/// sonra baseY'ye geri döner (pulse, 380 ms). LiveSunburst3D tarama
/// sırasında olduğu için gerçek drill backend yok; bu visual feedback.
function tickPulseAnim(state: SceneState) {
  const p = state.pulse;
  if (!p) return;
  const now = performance.now();
  const t = Math.min(1, (now - p.startTime) / p.duration);
  // Half-sin pulse: 0 → 1 → 0
  const eased = Math.sin(t * Math.PI);
  p.mesh.position.y = p.baseY + p.liftAmount * eased;
  if (t >= 1) {
    p.mesh.position.y = p.baseY;
    state.pulse = null;
  }
}

function clearWedges(state: SceneState) {
  for (const m of state.wedges) {
    state.scene.remove(m);
    m.geometry.dispose();
    if (Array.isArray(m.material)) {
      m.material.forEach((mat) => mat.dispose());
    } else {
      m.material.dispose();
    }
  }
  for (const lh of state.labels) {
    if (lh.div.parentNode) lh.div.parentNode.removeChild(lh.div);
  }
  state.wedges = [];
  state.labels = [];
  // Gemini review — animasyon state'i resetle; tickPulseAnim dispose
  // edilmiş mesh'e yazmasın + onPointerDown bloklanmasın.
  state.pulse = null;
}

function addWedgeLabel(
  state: SceneState,
  mesh: THREE.Mesh,
  name: string,
  midA: number,
  midR: number,
  topY: number,
  angleSpan: number,
) {
  const div = document.createElement("div");
  const truncated = name.length > 18 ? name.slice(0, 17) + "…" : name;
  div.textContent = truncated;
  div.className = "wedge-label3d";
  const isSmall = angleSpan < LABEL_ALWAYS_THRESHOLD_RAD;
  if (isSmall) div.classList.add("wedge-label-small");
  const labelObj = new CSS2DObject(div);
  // Mesh local: extrude rotateX(-PI/2) sonrası +Y'ye gider.
  // Wedge merkezi (XY original) → (cx, 0, -cy) local; top yüzü +Y.
  labelObj.position.set(Math.cos(midA) * midR, topY + 1.5, -Math.sin(midA) * midR);
  mesh.add(labelObj);
  // Cache world position için mesh matrix güncelle.
  mesh.updateMatrixWorld(true);
  const worldPos = new THREE.Vector3();
  labelObj.getWorldPosition(worldPos);
  state.labels.push({ div, mesh, worldPos, isSmall });
}

function rebuildWedges(state: SceneState, tree: PartialNode[]) {
  clearWedges(state);
  const root = tree.find((n) => n.depth === 0);
  if (!root || root.aggregate_size <= 0) {
    isEmpty.value = true;
    return;
  }
  const total = root.aggregate_size;
  const depth1 = tree
    .filter((n) => n.depth === 1 && n.parent === root.id && n.aggregate_size > 0)
    .sort((a, b) => b.aggregate_size - a.aggregate_size);
  if (depth1.length === 0) {
    isEmpty.value = true;
    return;
  }
  isEmpty.value = false;

  let angle = 0;
  const parentSlots = new Map<number, { a1: number; a2: number; idx: number }>();

  for (let i = 0; i < depth1.length; i++) {
    const node = depth1[i];
    const ratio = node.aggregate_size / total;
    const a1 = angle;
    const a2 = angle + ratio * Math.PI * 2;
    angle = a2;
    parentSlots.set(node.id, { a1, a2, idx: i });

    const a1g = a1 + GAP_RAD / 2;
    const a2g = a2 - GAP_RAD / 2;
    if (a2g <= a1g) continue;

    const shape = buildWedgeShape(a1g, a2g, RING1_INNER, RING1_OUTER);
    const geo = new THREE.ExtrudeGeometry(shape, {
      depth: RING_HEIGHT,
      bevelEnabled: true,
      bevelThickness: 1.2,
      bevelSize: 0.8,
      bevelSegments: 2,
      curveSegments: Math.max(8, Math.floor((a2g - a1g) * 24)),
    });
    geo.rotateX(-Math.PI / 2);

    const color = colorFor(i);
    const mat = new THREE.MeshStandardMaterial({
      color,
      roughness: 0.45,
      metalness: 0.15,
      emissive: 0x000000,
    });
    const mesh = new THREE.Mesh(geo, mat);
    mesh.position.y = RING1_Y;
    mesh.userData = {
      node,
      baseColor: color,
      baseY: RING1_Y,
    };
    state.scene.add(mesh);
    state.wedges.push(mesh);
    addWedgeLabel(
      state,
      mesh,
      node.name,
      (a1g + a2g) / 2,
      (RING1_INNER + RING1_OUTER) / 2,
      RING_HEIGHT,
      a2g - a1g,
    );
  }

  // Depth 2 — parent slot içinde dağıt, koyulaştırılmış renk.
  const depth2ByParent = new Map<number, PartialNode[]>();
  for (const n of tree) {
    if (n.depth !== 2 || n.parent === null || n.aggregate_size <= 0) continue;
    if (!parentSlots.has(n.parent)) continue;
    const arr = depth2ByParent.get(n.parent) ?? [];
    arr.push(n);
    depth2ByParent.set(n.parent, arr);
  }
  for (const [pid, kids] of depth2ByParent.entries()) {
    const slot = parentSlots.get(pid)!;
    const parentNode = depth1.find((p) => p.id === pid);
    if (!parentNode || parentNode.aggregate_size <= 0) continue;
    const sorted = [...kids].sort((a, b) => b.aggregate_size - a.aggregate_size);
    let kidAngle = slot.a1;
    const slotSpan = slot.a2 - slot.a1;
    for (const k of sorted) {
      const ratio = k.aggregate_size / parentNode.aggregate_size;
      const span = ratio * slotSpan;
      const a1 = kidAngle;
      const a2 = kidAngle + span;
      kidAngle = a2;
      const a1g = a1 + GAP_RAD / 2;
      const a2g = a2 - GAP_RAD / 2;
      if (a2g <= a1g) continue;

      const shape = buildWedgeShape(a1g, a2g, RING2_INNER, RING2_OUTER);
      const geo = new THREE.ExtrudeGeometry(shape, {
        depth: RING_HEIGHT,
        bevelEnabled: true,
        bevelThickness: 1.2,
        bevelSize: 0.8,
        bevelSegments: 2,
        curveSegments: Math.max(8, Math.floor((a2g - a1g) * 24)),
      });
      geo.rotateX(-Math.PI / 2);

      const color = darken(colorFor(slot.idx), 0.2);
      const mat = new THREE.MeshStandardMaterial({
        color,
        roughness: 0.5,
        metalness: 0.1,
        emissive: 0x000000,
      });
      const mesh = new THREE.Mesh(geo, mat);
      mesh.position.y = RING2_Y;
      mesh.userData = {
        node: k,
        baseColor: color,
        baseY: RING2_Y,
      };
      state.scene.add(mesh);
      state.wedges.push(mesh);
      addWedgeLabel(
        state,
        mesh,
        k.name,
        (a1g + a2g) / 2,
        (RING2_INNER + RING2_OUTER) / 2,
        RING_HEIGHT,
        a2g - a1g,
      );
    }
  }
}

/// Animation loop'tan çağrılır — her frame küçük label'ların görünürlüğünü
/// kamera mesafesine ve hover state'e göre günceller.
function updateLabelVisibility(state: SceneState) {
  const camPos = state.camera.position;
  for (const lh of state.labels) {
    if (!lh.isSmall) continue; // büyük her zaman görünür
    const hovered = state.hovered === lh.mesh;
    const dist = camPos.distanceTo(lh.worldPos);
    const near = dist < LABEL_NEAR_DISTANCE;
    if (hovered || near) {
      lh.div.classList.add("wedge-label-visible");
    } else {
      lh.div.classList.remove("wedge-label-visible");
    }
  }
}

function onResize(state: SceneState, canvas: HTMLCanvasElement) {
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  if (w === 0 || h === 0) return;
  state.camera.aspect = w / h;
  state.camera.updateProjectionMatrix();
  state.renderer.setSize(w, h, false);
  state.labelRenderer.setSize(w, h);
}

function setHover(state: SceneState, next: THREE.Mesh | null) {
  if (state.hovered === next) return;
  if (state.hovered) {
    const mat = state.hovered.material as THREE.MeshStandardMaterial;
    mat.emissive.setHex(0x000000);
    state.hovered.position.y = state.hovered.userData.baseY;
  }
  state.hovered = next;
  if (next) {
    const mat = next.material as THREE.MeshStandardMaterial;
    mat.emissive.setHex(0x222222);
    next.position.y = (next.userData.baseY as number) + 3;
    const node = next.userData.node as PartialNode;
    hoveredLabel.value = `${node.is_dir ? "📁" : "📄"} ${node.name} · ${formatBytes(node.aggregate_size)}`;
  } else {
    hoveredLabel.value = "";
  }
}

function onPointerMove(state: SceneState, canvas: HTMLCanvasElement, e: PointerEvent) {
  const rect = canvas.getBoundingClientRect();
  state.pointer.x = ((e.clientX - rect.left) / rect.width) * 2 - 1;
  state.pointer.y = -((e.clientY - rect.top) / rect.height) * 2 + 1;
  state.raycaster.setFromCamera(state.pointer, state.camera);
  const hits = state.raycaster.intersectObjects(state.wedges, false);
  if (hits.length > 0) {
    setHover(state, hits[0].object as THREE.Mesh);
  } else {
    setHover(state, null);
  }
}

function onPointerDown(state: SceneState, e: PointerEvent) {
  if (e.button !== 0 || !state.hovered || state.pulse) return;
  state.pulse = {
    mesh: state.hovered,
    startTime: performance.now(),
    duration: 380,
    baseY: state.hovered.userData.baseY as number,
    liftAmount: 14,
  };
}

let resizeObs: ResizeObserver | null = null;
let pointerMoveHandler: ((e: PointerEvent) => void) | null = null;
let pointerDownHandler: ((e: PointerEvent) => void) | null = null;

onMounted(() => {
  if (!canvasRef.value || !labelLayerRef.value) return;
  const state = buildScene(canvasRef.value, labelLayerRef.value);
  sceneState.value = state;

  if (props.partialTree && props.partialTree.length > 0) {
    rebuildWedges(state, props.partialTree);
  }

  const animate = () => {
    state.controls.update();
    tickPulseAnim(state);
    updateLabelVisibility(state);
    state.renderer.render(state.scene, state.camera);
    state.labelRenderer.render(state.scene, state.camera);
    state.animation = requestAnimationFrame(animate);
  };
  state.animation = requestAnimationFrame(animate);

  resizeObs = new ResizeObserver(() => {
    if (canvasRef.value) onResize(state, canvasRef.value);
  });
  resizeObs.observe(canvasRef.value);

  pointerMoveHandler = (e) => onPointerMove(state, canvasRef.value!, e);
  canvasRef.value.addEventListener("pointermove", pointerMoveHandler);
  pointerDownHandler = (e) => onPointerDown(state, e);
  canvasRef.value.addEventListener("pointerdown", pointerDownHandler);
});

onBeforeUnmount(() => {
  const state = sceneState.value;
  if (state) {
    if (state.animation !== null) cancelAnimationFrame(state.animation);
    clearWedges(state);
    // Gemini review — center disk geo/mat + labelRenderer + WebGL context
    // temizliği (GPU sızıntısı).
    state.scene.remove(state.center);
    state.center.geometry.dispose();
    (state.center.material as THREE.MeshStandardMaterial).dispose();
    state.controls.dispose();
    state.renderer.dispose();
    state.labelRenderer.domElement.remove();
    state.renderer.forceContextLoss();
  }
  if (resizeObs) resizeObs.disconnect();
  if (canvasRef.value && pointerMoveHandler) {
    canvasRef.value.removeEventListener("pointermove", pointerMoveHandler);
  }
  if (canvasRef.value && pointerDownHandler) {
    canvasRef.value.removeEventListener("pointerdown", pointerDownHandler);
  }
});

watch(
  () => props.partialTree,
  (next) => {
    const state = sceneState.value;
    if (state && next && next.length > 0) {
      rebuildWedges(state, next);
    }
  },
  { deep: false },
);
</script>

<template>
  <div class="live3d-wrap">
    <canvas ref="canvasRef" class="live3d-canvas" />
    <div ref="labelLayerRef" class="live3d-labels"></div>
    <div class="live3d-hud" v-if="hoveredLabel">{{ hoveredLabel }}</div>
    <div v-if="isEmpty" class="live3d-empty">
      <div class="empty-disk"></div>
      <p class="empty-text">{{ emptyMessage ?? "İlk veriler bekleniyor…" }}</p>
    </div>
    <div v-else class="live3d-hint">🖱 sürükle = döndür · tekerlek = zoom</div>
  </div>
</template>

<style scoped>
.live3d-wrap {
  position: relative;
  width: 360px;
  height: 360px;
  border-radius: 10px;
  overflow: hidden;
  flex-shrink: 0;
  background: var(--bg);
  border: 1px solid var(--border);
}

.live3d-canvas {
  width: 100%;
  height: 100%;
  display: block;
  cursor: grab;
  touch-action: none;
}

.live3d-canvas:active {
  cursor: grabbing;
}

/* CSS2DRenderer DOM layer — pointer-events: none ile altındaki canvas
   tıklamaları yakalamayı korur. */
.live3d-labels {
  position: absolute !important;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
  overflow: hidden;
}

/* Dilim adı label'ı — büyük dilimde her zaman görünür. */
.live3d-labels :deep(.wedge-label3d) {
  font-size: 10px;
  color: #fff;
  background: rgba(0, 0, 0, 0.55);
  padding: 1px 6px;
  border-radius: 4px;
  font-family: ui-monospace, "Cascadia Code", "Consolas", monospace;
  white-space: nowrap;
  transform: translate(-50%, -120%);
  text-shadow: 0 1px 2px rgba(0, 0, 0, 0.7);
  letter-spacing: 0.01em;
  pointer-events: none;
  user-select: none;
}

/* İnce dilim — varsayılan gizli, hover veya yakın kamera mesafesinde
   `.wedge-label-visible` JS tarafından eklenir → fade-in. */
.live3d-labels :deep(.wedge-label-small) {
  opacity: 0;
  transform: translate(-50%, -120%) scale(0.85);
  transition:
    opacity 0.18s ease,
    transform 0.18s ease;
}

.live3d-labels :deep(.wedge-label-small.wedge-label-visible) {
  opacity: 1;
  transform: translate(-50%, -120%) scale(1);
}

.live3d-hud {
  position: absolute;
  top: 8px;
  left: 8px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 4px 8px;
  font-size: 11px;
  color: var(--fg);
  font-family: ui-monospace, monospace;
  box-shadow: 0 4px 10px var(--shadow);
  max-width: calc(100% - 16px);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  pointer-events: none;
}

.live3d-hint {
  position: absolute;
  bottom: 6px;
  right: 10px;
  font-size: 9px;
  color: var(--muted);
  pointer-events: none;
  opacity: 0.75;
}

.live3d-empty {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--muted);
  pointer-events: none;
}

.empty-disk {
  width: 100px;
  height: 100px;
  border: 2px dashed var(--border);
  border-radius: 50%;
  animation: empty-pulse 1.6s ease-in-out infinite;
}

@keyframes empty-pulse {
  0%,
  100% {
    opacity: 0.4;
    transform: scale(0.95);
  }
  50% {
    opacity: 0.8;
    transform: scale(1);
  }
}

.empty-text {
  margin: 0;
  font-size: 12px;
  font-style: italic;
}
</style>
