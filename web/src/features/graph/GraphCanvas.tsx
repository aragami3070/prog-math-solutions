import { useEffect, useRef } from "react";
import { drag } from "d3-drag";
import { forceCollide, forceLink, forceManyBody, forceSimulation, forceX, forceY, type SimulationLinkDatum, type SimulationNodeDatum } from "d3-force";
import { pointer, select } from "d3-selection";
import { zoom, zoomIdentity } from "d3-zoom";
import type { OverviewEdge, OverviewNode } from "../../lib/types";

export const NODE_COLORS: Record<string, string> = { internals: "#657d40", rfc: "#a26748" };

interface SimNode extends SimulationNodeDatum, OverviewNode {
  r: number;
}
type SimLink = SimulationLinkDatum<SimNode> & { occurrences: number };
interface DragSubject {
  x: number;
  y: number;
  node: SimNode;
}

interface Props {
  nodes: OverviewNode[];
  edges: OverviewEdge[];
  selectedId: string;
  /** Centre the view on this node whenever the object changes (a new object means a new request). */
  focus: { id: string } | null;
  /** A click on a node (id) or on the empty background (null). */
  onSelect: (id: string | null) => void;
}

const FONT = "Inter, ui-sans-serif, system-ui, sans-serif";
// Keep only the most recent graph, and use the full serialized input rather than a
// lossy hash: order also matters to d3's initial positions and force iteration.
const LAYOUT_VERSION = 1;
let layoutCache: { key: string; positions: { x: number; y: number }[] } | null = null;

/**
 * The reference graph as a picture: a force layout on a canvas. Every node is a document, sized
 * by its links and coloured by source. Click selects, double click opens the original, drag
 * moves a node, wheel zooms, dragging the background pans. Labels appear for the best-linked
 * nodes and more of them as you zoom in; the node under the pointer (or the selected one) and
 * its neighbours are always labelled, the rest dims.
 */
export function GraphCanvas({ nodes, edges, selectedId, focus, onSelect }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const live = useRef({ selectedId, onSelect });
  live.current = { selectedId, onSelect };
  const api = useRef<{ redraw(): void; center(id: string): void } | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const key = JSON.stringify([LAYOUT_VERSION, nodes, edges]);
    const cached = layoutCache?.key === key ? layoutCache.positions : null;
    const simNodes: SimNode[] = nodes.map((node, i) => ({
      ...node, r: 3 + Math.sqrt(node.degree) * 2,
      ...(cached ? { x: cached[i].x, y: cached[i].y } : {}),
    }));
    let layoutComplete = !!cached;
    let layoutTimer: ReturnType<typeof setTimeout> | undefined;
    const saveLayout = () => {
      if (!layoutComplete) return;
      layoutCache = { key, positions: simNodes.map(node => ({ x: node.x ?? 0, y: node.y ?? 0 })) };
    };
    const byId = new Map(simNodes.map(node => [node.id, node]));
    const links: SimLink[] = [];
    const neighbours = new Map<string, Set<string>>();
    const remember = (a: string, b: string) => {
      const set = neighbours.get(a) ?? new Set<string>();
      set.add(b);
      neighbours.set(a, set);
    };
    for (const edge of edges) {
      const source = byId.get(edge.source);
      const target = byId.get(edge.target);
      if (!source || !target) continue;
      links.push({ source, target, occurrences: edge.occurrences });
      remember(source.id, target.id);
      remember(target.id, source.id);
    }

    const labelOrder = [...simNodes].sort((a, b) => b.degree - a.degree);
    let transform = zoomIdentity;
    let hover: SimNode | null = null;
    let width = 0;
    let height = 0;
    let frame = 0;

    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(() => { frame = 0; draw(); });
    };

    const draw = () => {
      const k = transform.k;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, width, height);
      ctx.translate(transform.x, transform.y);
      ctx.scale(k, k);
      const focusNode = hover ?? byId.get(live.current.selectedId) ?? null;
      const near = focusNode ? neighbours.get(focusNode.id) : undefined;
      const lit = (id: string) => !focusNode || id === focusNode.id || (near?.has(id) ?? false);

      ctx.lineWidth = 1 / k;
      for (const link of links) {
        const s = link.source as SimNode;
        const t = link.target as SimNode;
        const on = focusNode !== null && (s.id === focusNode.id || t.id === focusNode.id);
        ctx.strokeStyle = on ? "rgba(38,61,46,0.85)" : focusNode ? "rgba(114,123,108,0.06)" : "rgba(114,123,108,0.3)";
        ctx.beginPath();
        ctx.moveTo(s.x ?? 0, s.y ?? 0);
        ctx.lineTo(t.x ?? 0, t.y ?? 0);
        ctx.stroke();
      }

      for (const node of simNodes) {
        ctx.globalAlpha = lit(node.id) ? 1 : 0.15;
        ctx.fillStyle = NODE_COLORS[node.source] ?? "#727b6c";
        ctx.beginPath();
        ctx.arc(node.x ?? 0, node.y ?? 0, node.r, 0, Math.PI * 2);
        ctx.fill();
        if (node.id === live.current.selectedId) {
          ctx.globalAlpha = 1;
          ctx.lineWidth = 2.5 / k;
          ctx.strokeStyle = "#252d29";
          ctx.stroke();
        }
      }

      ctx.globalAlpha = 1;
      ctx.font = `${12 / k}px ${FONT}`;
      ctx.textAlign = "center";
      ctx.textBaseline = "top";
      ctx.lineJoin = "round";
      // Best-linked first; a label that would cover one already drawn is skipped, so the dense
      // centre stays readable and zooming in reveals the rest.
      const threshold = k < 0.5 ? 8 : k < 0.9 ? 4 : k < 1.6 ? 2 : 0;
      const taken: [number, number, number, number][] = [];
      for (const node of labelOrder) {
        const show = focusNode ? lit(node.id) : node.degree >= threshold;
        if (!show) continue;
        const text = node.title.length > 44 ? `${node.title.slice(0, 43)}…` : node.title;
        const x = node.x ?? 0;
        const y = (node.y ?? 0) + node.r + 2 / k;
        const half = ctx.measureText(text).width / 2;
        const box: [number, number, number, number] = [x - half, y, x + half, y + 13 / k];
        const emphasised = node === focusNode;
        if (!emphasised && taken.some(t => box[0] < t[2] && box[2] > t[0] && box[1] < t[3] && box[3] > t[1])) continue;
        taken.push(box);
        ctx.lineWidth = 3.5 / k;
        ctx.strokeStyle = "rgba(246,247,243,0.92)";
        ctx.strokeText(text, x, y);
        ctx.fillStyle = "#252d29";
        ctx.fillText(text, x, y);
      }
    };

    const nodeAt = (px: number, py: number): SimNode | null => {
      if (!layoutComplete) return null;
      const [x, y] = transform.invert([px, py]);
      let best: SimNode | null = null;
      let bestDistance = Infinity;
      for (const node of simNodes) {
        const distance = Math.hypot((node.x ?? 0) - x, (node.y ?? 0) - y);
        if (distance <= node.r + 4 / transform.k && distance < bestDistance) {
          best = node;
          bestDistance = distance;
        }
      }
      return best;
    };

    const zoomBehavior = zoom<HTMLCanvasElement, unknown>()
      .scaleExtent([0.08, 8])
      .filter(event => event.type === "wheel" || !nodeAt(...pointer(event, canvas)))
      .on("zoom", event => { transform = event.transform; schedule(); });
    const view = select(canvas);
    const moveTo = (x: number, y: number, k: number) =>
      view.call(zoomBehavior.transform, zoomIdentity.translate(width / 2 - x * k, height / 2 - y * k).scale(k));

    const resize = () => {
      const rect = canvas.getBoundingClientRect();
      width = rect.width;
      height = rect.height;
      canvas.width = Math.round(width * dpr);
      canvas.height = Math.round(height * dpr);
      schedule();
    };
    resize();

    const simulation = forceSimulation<SimNode>(simNodes)
      .force("link", forceLink<SimNode, SimLink>(links).distance(28).strength(0.5))
      .force("charge", forceManyBody<SimNode>().strength(-45).distanceMax(250))
      .force("collide", forceCollide<SimNode>().radius(node => node.r + 1.5))
      .force("x", forceX<SimNode>(0).strength(0.05))
      .force("y", forceY<SimNode>(0).strength(0.05))
      .stop();

    view.call(zoomBehavior).on("dblclick.zoom", null);
    const fitGraph = () => {
      if (simNodes.length > 0) {
        const xs = simNodes.map(node => node.x ?? 0);
        const ys = simNodes.map(node => node.y ?? 0);
        const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
        const fit = Math.min(width / Math.max(x1 - x0, 1), height / Math.max(y1 - y0, 1)) * 0.9;
        moveTo((x0 + x1) / 2, (y0 + y1) / 2, Math.min(Math.max(fit, 0.1), 2));
      }
      const first = byId.get(live.current.selectedId);
      if (first) moveTo(first.x ?? 0, first.y ?? 0, 1.6);
    };
    if (cached) fitGraph();

    const dragger = drag<HTMLCanvasElement, unknown, DragSubject>()
      .subject(event => {
        const node = nodeAt(event.x, event.y);
        // No subject means "not a node": the gesture is left to the background pan.
        return node ? { x: event.x, y: event.y, node } : (undefined as unknown as DragSubject);
      })
      .on("start", event => {
        simulation.alphaTarget(0.25).restart();
        event.subject.node.fx = event.subject.node.x;
        event.subject.node.fy = event.subject.node.y;
      })
      .on("drag", event => {
        const node = event.subject.node;
        node.fx = transform.invertX(event.x);
        node.fy = transform.invertY(event.y);
        moved = moved || Math.hypot(event.x - event.subject.x, event.y - event.subject.y) > 4;
      })
      .on("end", event => {
        simulation.alphaTarget(0);
        const node = event.subject.node;
        node.fx = null;
        node.fy = null;
        if (!moved) live.current.onSelect(node.id);
        moved = false;
      });
    let moved = false;
    view.call(dragger);

    const onMove = (event: PointerEvent) => {
      const found = nodeAt(...pointer(event, canvas));
      if (found !== hover) {
        hover = found;
        canvas.style.cursor = found ? "pointer" : "grab";
        schedule();
      }
    };
    const onLeave = () => { hover = null; schedule(); };
    const onClick = (event: MouseEvent) => {
      if (!nodeAt(...pointer(event, canvas))) live.current.onSelect(null);
    };
    const onDouble = (event: MouseEvent) => {
      const found = nodeAt(...pointer(event, canvas));
      if (found) window.open(found.url, "_blank", "noopener,noreferrer");
    };
    canvas.addEventListener("pointermove", onMove);
    canvas.addEventListener("pointerleave", onLeave);
    canvas.addEventListener("click", onClick);
    canvas.addEventListener("dblclick", onDouble);
    const observer = new ResizeObserver(resize);
    observer.observe(canvas);
    simulation.on("tick", schedule).on("end", saveLayout);
    if (!cached) {
      let remaining = 300;
      const calculate = () => {
        // Yield between small batches so input and painting can run during layout.
        const count = Math.min(5, remaining);
        simulation.tick(count);
        remaining -= count;
        if (remaining > 0) {
          layoutTimer = setTimeout(calculate, 0);
        } else {
          layoutComplete = true;
          saveLayout();
          fitGraph();
          schedule();
        }
      };
      layoutTimer = setTimeout(calculate, 0);
    }

    api.current = {
      redraw: schedule,
      center: id => {
        const node = byId.get(id);
        if (node) moveTo(node.x ?? 0, node.y ?? 0, Math.max(transform.k, 1.6));
      },
    };
    schedule();

    return () => {
      api.current = null;
      simulation.stop();
      if (layoutTimer !== undefined) clearTimeout(layoutTimer);
      saveLayout();
      cancelAnimationFrame(frame);
      observer.disconnect();
      canvas.removeEventListener("pointermove", onMove);
      canvas.removeEventListener("pointerleave", onLeave);
      canvas.removeEventListener("click", onClick);
      canvas.removeEventListener("dblclick", onDouble);
      view.on(".zoom", null).on(".drag", null);
    };
  }, [nodes, edges]);

  useEffect(() => { api.current?.redraw(); }, [selectedId]);
  useEffect(() => { if (focus) api.current?.center(focus.id); }, [focus]);

  return <canvas ref={canvasRef} className="graph-canvas" role="img" aria-label="Граф ссылок между документами; список связей выбранного документа ниже" />;
}
