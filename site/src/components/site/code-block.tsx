import * as React from "react"
import { AnimatePresence, motion } from "motion/react"
import { CheckIcon, CopyIcon } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { usePrefs } from "@/hooks/use-prefs"
import { cn } from "@/lib/utils"

async function writeClipboard(text: string) {
  if (navigator.clipboard && window.isSecureContext) return navigator.clipboard.writeText(text)
  const ta = document.createElement("textarea")
  ta.value = text
  ta.style.position = "fixed"
  ta.style.opacity = "0"
  document.body.appendChild(ta)
  ta.select()
  const ok = document.execCommand("copy")
  ta.remove()
  if (!ok) throw new Error("copy failed")
}

export function CopyButton({ text, className }: { text: string; className?: string }) {
  const { t } = usePrefs()
  const [done, setDone] = React.useState(false)
  const onCopy = async () => {
    try {
      await writeClipboard(text)
      setDone(true)
      toast.success(t.examples.copied)
      setTimeout(() => setDone(false), 1400)
    } catch {
      /* clipboard unavailable: leave the text selectable */
    }
  }
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t.examples.copy}
          onClick={onCopy}
          className={cn("text-current/60 hover:bg-white/10 hover:text-current", className)}
        >
          <AnimatePresence mode="popLayout" initial={false}>
            <motion.span
              key={done ? "done" : "copy"}
              initial={{ opacity: 0, scale: 0.6, rotate: -30 }}
              animate={{ opacity: 1, scale: 1, rotate: 0 }}
              exit={{ opacity: 0, scale: 0.6, rotate: 30 }}
              transition={{ duration: 0.18 }}
              className="grid place-items-center"
            >
              {done ? <CheckIcon className="text-term-prompt" /> : <CopyIcon className="size-3.5" />}
            </motion.span>
          </AnimatePresence>
        </Button>
      </TooltipTrigger>
      <TooltipContent>{t.examples.copy}</TooltipContent>
    </Tooltip>
  )
}

/** Terminal-colored code block. `lines` are shell commands (get a prompt) unless `raw`. */
export function CodeBlock({
  lines,
  raw,
  title,
  className,
}: {
  lines: string[] | React.ReactNode
  raw?: boolean
  title?: string
  className?: string
}) {
  const text = Array.isArray(lines) ? lines.join("\n") : ""
  return (
    <div
      className={cn(
        "relative rounded-md border border-rule-strong bg-term-bg font-mono text-[12.5px] leading-[1.75] text-term-fg",
        className
      )}
    >
      {title && (
        <div className="border-b border-white/10 px-4 py-1.5 text-[11px] text-term-dim">{title}</div>
      )}
      <pre className="scrollbar-thin overflow-x-auto px-4 py-3 pr-12 break-words whitespace-pre-wrap">
        {Array.isArray(lines)
          ? lines.map((l, i) => (
              <div key={i}>
                {!raw && <span className="text-term-prompt select-none">$ </span>}
                {l}
              </div>
            ))
          : lines}
      </pre>
      {text && <CopyButton text={text} className="absolute top-1.5 right-1.5 text-term-fg" />}
    </div>
  )
}
