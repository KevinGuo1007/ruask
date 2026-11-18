import * as React from "react"
import { AnimatePresence, motion, useInView, useReducedMotion } from "motion/react"

import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { scenes, type Scene } from "@/content"
import { usePrefs } from "@/hooks/use-prefs"
import { cn } from "@/lib/utils"

const SPINNER = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"

/* `inline code` → yellow italic, inside prose */
function prose(text: string, base: string) {
  return text.split(/(`[^`]+`)/g).map((part, i) =>
    part.startsWith("`") && part.endsWith("`") ? (
      <span key={i} className="text-term-code italic">
        {part.slice(1, -1)}
      </span>
    ) : (
      <span key={i} className={base}>
        {part}
      </span>
    )
  )
}

/* {{placeholder}} → italic + underline, braces dropped (ruask v0.4 renderer) */
function command(text: string) {
  return text.split(/(\{\{.*?\}\})/g).map((part, i) =>
    part.startsWith("{{") ? (
      <span key={i} className="italic underline decoration-term-cmd/60 underline-offset-[3px]">
        {part.slice(2, -2)}
      </span>
    ) : (
      <span key={i}>{part}</span>
    )
  )
}

function Line({ line, n }: { line: string; n: number }) {
  const head = line[0]
  const body = line.slice(2)
  switch (head) {
    case "#":
      return <div className="font-medium text-term-title">{body}</div>
    case ">":
      return <div className="pl-[2ch]">{prose(body, "text-term-fg")}</div>
    case "-":
      return (
        <div className="mt-[1lh] pl-[2ch]">
          <span className="text-term-dim">{n} </span>
          {prose(body, "text-term-use")}
        </div>
      )
    case "`":
      return <div className="pl-[4ch] text-term-cmd">{command(line.slice(1, -1))}</div>
    case "!":
      return <div className="pl-[4ch] text-term-danger">{body}</div>
    case "~":
      return <div className="mt-[1lh] text-term-dim">{body}</div>
    default:
      return <div>{line}</div>
  }
}

function Output({ scene, count }: { scene: Scene; count: number }) {
  let n = 0
  return (
    <>
      {scene.lines.slice(0, count).map((line, i) => {
        if (line.startsWith("- ")) n += 1
        return (
          <motion.div
            key={`${scene.id}-${i}`}
            initial={{ opacity: 0, x: -4 }}
            animate={{ opacity: 1, x: 0 }}
            transition={{ duration: 0.18 }}
          >
            <Line line={line} n={n} />
          </motion.div>
        )
      })}
    </>
  )
}

type Phase = "typing" | "waiting" | "streaming" | "done"

export function Terminal({ className }: { className?: string }) {
  const { lang } = usePrefs()
  const list = scenes[lang]
  const reduce = useReducedMotion()
  const ref = React.useRef<HTMLDivElement>(null)
  const bodyRef = React.useRef<HTMLDivElement>(null)
  const inView = useInView(ref, { margin: "-10% 0px" })

  const [index, setIndex] = React.useState(0)
  const [typed, setTyped] = React.useState(0)
  const [count, setCount] = React.useState(0)
  const [phase, setPhase] = React.useState<Phase>("typing")
  const [spin, setSpin] = React.useState(0)
  const scene = list[index] ?? list[0]

  // Drive one scene: type the command, show the waiting hint, stream lines.
  React.useEffect(() => {
    if (reduce) {
      setTyped(scene.cmd.length)
      setCount(scene.lines.length)
      setPhase("done")
      return
    }
    if (!inView) return
    let cancelled = false
    const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))
    ;(async () => {
      setTyped(0)
      setCount(0)
      setPhase("typing")
      await sleep(450)
      for (let i = 1; i <= scene.cmd.length; i++) {
        if (cancelled) return
        setTyped(i)
        await sleep(26 + Math.random() * 40)
      }
      await sleep(260)
      if (scene.wait) {
        setPhase("waiting")
        await sleep(900)
      } else {
        await sleep(120)
      }
      if (cancelled) return
      setPhase("streaming")
      for (let i = 1; i <= scene.lines.length; i++) {
        if (cancelled) return
        setCount(i)
        await sleep(scene.wait ? 150 + Math.random() * 180 : 40)
      }
      setPhase("done")
      await sleep(4200)
      if (!cancelled) setIndex((i) => (i + 1) % list.length)
    })()
    return () => {
      cancelled = true
    }
  }, [scene, inView, reduce, list.length])

  React.useEffect(() => {
    if (phase !== "waiting") return
    const id = setInterval(() => setSpin((s) => (s + 1) % SPINNER.length), 80)
    return () => clearInterval(id)
  }, [phase])

  React.useEffect(() => {
    const el = bodyRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [count])

  return (
    <div
      ref={ref}
      className={cn(
        "relative overflow-hidden rounded-md border border-rule-strong bg-term-bg text-term-fg",
        className
      )}
    >
      <Tabs value={scene.id} onValueChange={(v) => setIndex(list.findIndex((s) => s.id === v))}>
        <div className="flex items-center gap-3 border-b border-white/10 px-3 py-2">
          <div className="flex gap-1.5" aria-hidden>
            <span className="size-2.5 rounded-full bg-white/15" />
            <span className="size-2.5 rounded-full bg-white/15" />
            <span className="size-2.5 rounded-full bg-white/15" />
          </div>
          <TabsList
            aria-label="Demo scenes"
            className="scrollbar-thin -my-1 overflow-x-auto text-white/45 [&_[data-state=active]]:!border-white/15 [&_[data-state=active]]:!bg-white/[0.06] [&_[data-state=active]]:!text-white/90 [&_button:hover]:text-white/80"
          >
            {list.map((s) => (
              <TabsTrigger key={s.id} value={s.id}>
                {s.tab}
              </TabsTrigger>
            ))}
          </TabsList>
        </div>
      </Tabs>

      <div
        ref={bodyRef}
        className="scrollbar-thin h-[372px] overflow-y-auto px-4 pt-4 pb-5 font-mono text-[12.5px] leading-[1.65] sm:text-[13px]"
        aria-live="off"
      >
        <AnimatePresence mode="wait">
          <motion.div
            key={`${lang}-${scene.id}`}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="break-words whitespace-pre-wrap"
          >
            <div>
              <span className="text-term-prompt">❯ </span>
              <span>{scene.cmd.slice(0, typed)}</span>
              {phase === "typing" && <Caret />}
            </div>
            {phase === "waiting" && scene.wait && (
              <div className="text-term-dim">
                {SPINNER[spin]}
                {scene.wait.slice(1)}
              </div>
            )}
            <Output scene={scene} count={count} />
            {phase === "done" && (
              <div className="mt-[1lh]">
                <span className="text-term-prompt">❯ </span>
                <Caret />
              </div>
            )}
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  )
}

function Caret() {
  return (
    <motion.span
      aria-hidden
      className="inline-block h-[1.1em] w-[0.6ch] translate-y-[0.15em] bg-term-fg/80"
      animate={{ opacity: [1, 1, 0, 0] }}
      transition={{ duration: 1, repeat: Infinity, times: [0, 0.5, 0.5, 1] }}
    />
  )
}
