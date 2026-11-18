import * as React from "react"
import { AnimatePresence, motion, useScroll, useSpring } from "motion/react"
import { MoonIcon, SunIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { REPO } from "@/content"
import { usePrefs } from "@/hooks/use-prefs"

export function GithubMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 16 16" className={className} aria-hidden fill="currentColor">
      <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0 0 16 8c0-4.42-3.58-8-8-8z" />
    </svg>
  )
}

const LINKS = ["how", "features", "start", "examples"] as const

function useActiveSection() {
  const [active, setActive] = React.useState<string | null>(null)
  React.useEffect(() => {
    const update = () => {
      const line = window.innerHeight * 0.4
      let current: string | null = null
      for (const id of LINKS) {
        const el = document.getElementById(id)
        if (!el) continue
        const r = el.getBoundingClientRect()
        if (r.top <= line && r.bottom > line) current = id
      }
      setActive(current)
    }
    update()
    window.addEventListener("scroll", update, { passive: true })
    window.addEventListener("resize", update)
    return () => {
      window.removeEventListener("scroll", update)
      window.removeEventListener("resize", update)
    }
  }, [])
  return active
}

export function Nav() {
  const { t, lang, setLang, theme, toggleTheme } = usePrefs()
  const { scrollYProgress } = useScroll()
  const progress = useSpring(scrollYProgress, { stiffness: 200, damping: 30, restDelta: 0.001 })
  const active = useActiveSection()

  return (
    <header className="sticky top-0 z-50 border-b border-rule bg-background/85 backdrop-blur-md">
      <div className="mx-auto flex h-14 max-w-[1180px] items-center gap-6 px-5 sm:px-10">
        <a href="#top" className="flex items-center gap-2 font-mono text-[15px] font-medium tracking-tight">
          <span className="text-signal">❯</span>ruask
        </a>
        <nav className="hidden items-center gap-5 md:flex" aria-label="Primary">
          {LINKS.map((id) => (
            <Button key={id} variant="bracket" asChild aria-current={active === id}>
              <a href={`#${id}`}>{t.nav[id]}</a>
            </Button>
          ))}
        </nav>
        <div className="ml-auto flex items-center gap-2">
          <ToggleGroup
            type="single"
            variant="outline"
            size="sm"
            value={lang}
            onValueChange={(v) => v && setLang(v as "zh" | "en")}
            aria-label={t.toggles.lang}
          >
            <ToggleGroupItem value="zh" aria-label="中文">
              中
            </ToggleGroupItem>
            <ToggleGroupItem value="en" aria-label="English">
              EN
            </ToggleGroupItem>
          </ToggleGroup>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button variant="outline" size="icon-sm" onClick={toggleTheme} aria-label={t.toggles.theme}>
                <AnimatePresence mode="popLayout" initial={false}>
                  <motion.span
                    key={theme}
                    initial={{ rotate: -90, opacity: 0 }}
                    animate={{ rotate: 0, opacity: 1 }}
                    exit={{ rotate: 90, opacity: 0 }}
                    transition={{ duration: 0.25 }}
                    className="grid place-items-center"
                  >
                    {theme === "dark" ? <MoonIcon className="size-3.5" /> : <SunIcon className="size-3.5" />}
                  </motion.span>
                </AnimatePresence>
              </Button>
            </TooltipTrigger>
            <TooltipContent>{t.toggles.theme}</TooltipContent>
          </Tooltip>
          <Button variant="outline" size="icon-sm" asChild>
            <a href={REPO} aria-label="GitHub">
              <GithubMark className="size-3.5" />
            </a>
          </Button>
        </div>
      </div>
      <motion.div
        aria-hidden
        className="absolute inset-x-0 -bottom-px h-px origin-left bg-blueprint"
        style={{ scaleX: progress }}
      />
    </header>
  )
}
