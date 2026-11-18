import * as React from "react"
import { motion, useReducedMotion } from "motion/react"

import { cn } from "@/lib/utils"

/** Section kicker: "── 03 / 特性" in mono, the dash draws in. */
export function Kicker({ index, children }: { index?: string; children: React.ReactNode }) {
  const reduce = useReducedMotion()
  return (
    <div className="flex items-center gap-2.5 font-mono text-[12px] tracking-wide text-signal">
      <motion.span
        aria-hidden
        className="h-px w-7 origin-left bg-current"
        initial={reduce ? false : { scaleX: 0 }}
        whileInView={{ scaleX: 1 }}
        viewport={{ once: true, margin: "-10% 0px" }}
        transition={{ duration: 0.6, ease: [0.2, 0.7, 0.1, 1] }}
      />
      {index && <span className="text-ink-faint">{index} /</span>}
      <span>{children}</span>
    </div>
  )
}

/** Serif display heading. Latin goes italic; CJK stays upright via font-synthesis: none. */
export function Display({
  as: Tag = "h2",
  className,
  children,
}: {
  as?: "h1" | "h2" | "h3"
  className?: string
  children: React.ReactNode
}) {
  return (
    <Tag className={cn("font-serif font-light tracking-[-0.015em] text-balance", className)}>
      {children}
    </Tag>
  )
}

/** Fade + rise when scrolled into view; children can stagger via `delay`. */
export function Reveal({
  children,
  delay = 0,
  className,
  y = 14,
}: {
  children: React.ReactNode
  delay?: number
  className?: string
  y?: number
}) {
  const reduce = useReducedMotion()
  return (
    <motion.div
      className={className}
      initial={reduce ? false : { opacity: 0, y }}
      whileInView={{ opacity: 1, y: 0 }}
      viewport={{ once: true, margin: "-8% 0px" }}
      transition={{ duration: 0.6, delay, ease: [0.2, 0.7, 0.1, 1] }}
    >
      {children}
    </motion.div>
  )
}

/** "+" crosshair placed on a rail intersection. */
export function Cross({ className }: { className?: string }) {
  return (
    <span
      aria-hidden
      className={cn(
        "pointer-events-none absolute z-10 size-[9px] -translate-x-1/2 -translate-y-1/2 text-rule-strong",
        className
      )}
    >
      <span className="absolute top-1/2 left-0 h-px w-full -translate-y-1/2 bg-current" />
      <span className="absolute top-0 left-1/2 h-full w-px -translate-x-1/2 bg-current" />
    </span>
  )
}

/**
 * A page section framed by the blueprint rails: a full-width top rule with
 * crosshairs where it meets the two vertical rails of the content column.
 */
export function Section({
  id,
  className,
  innerClassName,
  children,
}: {
  id?: string
  className?: string
  innerClassName?: string
  children: React.ReactNode
}) {
  return (
    <section id={id} className={cn("relative border-t border-rule", className)}>
      <div className="relative mx-auto max-w-[1180px] border-x border-dashed border-rule">
        <Cross className="top-0 left-0" />
        <Cross className="top-0 left-full" />
        <div className={cn("px-5 py-20 sm:px-10 md:py-24", innerClassName)}>{children}</div>
      </div>
    </section>
  )
}
