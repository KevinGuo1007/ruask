import { motion, useReducedMotion } from "motion/react"

import { usePrefs } from "@/hooks/use-prefs"
import { cn } from "@/lib/utils"

type Box = { x: number; y: number; w: number; h: number; title: string; note: string; tone?: "local" | "model" | "check" }

/*
 * The routing diagram drawn like a blueprint: boxes are 1px panels, edges
 * draw themselves on scroll, and a pulse travels each path.
 */
export function FlowDiagram() {
  const { t } = usePrefs()
  const n = t.how.nodes
  const reduce = useReducedMotion()

  const boxes: Box[] = [
    { x: 10, y: 48, w: 180, h: 68, title: n.query, note: n.queryNote },
    { x: 250, y: 48, w: 190, h: 68, title: n.check, note: n.checkNote, tone: "check" },
    { x: 510, y: 48, w: 200, h: 68, title: n.local, note: n.localNote, tone: "local" },
    { x: 250, y: 214, w: 190, h: 68, title: n.ref, note: n.refNote, tone: "model" },
    { x: 510, y: 214, w: 200, h: 68, title: n.model, note: n.modelNote, tone: "model" },
    { x: 820, y: 131, w: 200, h: 68, title: n.render, note: n.renderNote },
  ]
  const edges: { d: string; tone: "local" | "model" | "plain"; label?: [number, number, string] }[] = [
    { d: "M190 82 H250", tone: "plain" },
    { d: "M440 82 H510", tone: "local", label: [475, 72, t.how.yes] },
    { d: "M345 116 V214", tone: "model", label: [357, 170, t.how.no] },
    { d: "M440 248 H510", tone: "model" },
    { d: "M710 82 H765 V165 H820", tone: "local" },
    { d: "M710 248 H765 V165", tone: "model" },
  ]
  const stroke = { local: "var(--signal)", model: "var(--blueprint)", plain: "var(--rule-strong)" }

  return (
    <>
      <svg
        viewBox="0 0 1030 300"
        className="hidden w-full md:block"
        role="img"
        aria-label={`${n.query} → ${n.check} → ${n.local} / ${n.ref} → ${n.model} → ${n.render}`}
      >
        <defs>
          <marker id="arrow" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" orient="auto">
            <path d="M0 0 L8 4 L0 8" fill="none" stroke="var(--ink-faint)" strokeWidth="1.2" />
          </marker>
        </defs>
        {edges.map((e, i) => (
          <g key={i}>
            <motion.path
              d={e.d}
              fill="none"
              stroke={stroke[e.tone]}
              strokeWidth={1.25}
              strokeDasharray={e.tone === "plain" ? "4 4" : undefined}
              markerEnd="url(#arrow)"
              initial={reduce ? false : { pathLength: 0, opacity: 0 }}
              whileInView={{ pathLength: 1, opacity: 1 }}
              viewport={{ once: true, margin: "-15% 0px" }}
              transition={{ duration: 0.7, delay: 0.25 + i * 0.18, ease: [0.2, 0.7, 0.1, 1] }}
            />
            {!reduce && e.tone !== "plain" && (
              <circle r="2.6" fill={stroke[e.tone]}>
                <animateMotion dur="2.6s" begin={`${1.6 + i * 0.3}s`} repeatCount="indefinite" path={e.d} />
              </circle>
            )}
            {e.label && (
              <text x={e.label[0]} y={e.label[1]} textAnchor="middle" className="fill-ink-faint font-mono text-[12px]">
                {e.label[2]}
              </text>
            )}
          </g>
        ))}
        {boxes.map((b, i) => (
          <motion.g
            key={i}
            initial={reduce ? false : { opacity: 0, y: 8 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: true, margin: "-15% 0px" }}
            transition={{ duration: 0.5, delay: i * 0.12 }}
          >
            <rect
              x={b.x}
              y={b.y}
              width={b.w}
              height={b.h}
              rx={3}
              fill="var(--paper-raised)"
              stroke={b.tone === "local" ? "var(--signal)" : b.tone === "model" ? "var(--blueprint)" : "var(--rule-strong)"}
              strokeDasharray={b.tone === "check" ? "5 4" : undefined}
            />
            <text x={b.x + 16} y={b.y + 29} className="fill-ink text-[15px] font-medium">
              {b.title}
            </text>
            <text x={b.x + 16} y={b.y + 51} className="fill-ink-faint font-mono text-[11.5px]">
              {b.note}
            </text>
          </motion.g>
        ))}
      </svg>

      {/* narrow screens: the same routing as a vertical list */}
      <ol className="grid gap-3 md:hidden">
        {[
          [n.query, n.queryNote, "plain"],
          [n.check, n.checkNote, "check"],
          [`${t.how.yes} → ${n.local}`, n.localNote, "local"],
          [`${t.how.no} → ${n.ref}`, n.refNote, "model"],
          [n.model, n.modelNote, "model"],
          [n.render, n.renderNote, "plain"],
        ].map(([title, note, tone], i) => (
          <li
            key={i}
            className={cn(
              "border bg-paper-raised px-4 py-3",
              tone === "local" && "border-signal/60",
              tone === "model" && "border-blueprint/60",
              tone === "check" && "border-dashed border-rule-strong",
              tone === "plain" && "border-rule-strong"
            )}
          >
            <div className="text-[15px] font-medium">{title}</div>
            <div className="font-mono text-[12px] text-ink-faint">{note}</div>
          </li>
        ))}
      </ol>
    </>
  )
}
