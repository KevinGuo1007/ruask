import * as React from "react"

import { copy, type Copy, type Lang } from "@/content"

type Theme = "light" | "dark"
type Prefs = {
  lang: Lang
  t: Copy
  setLang: (l: Lang) => void
  theme: Theme
  toggleTheme: () => void
}

const PrefsContext = React.createContext<Prefs | null>(null)

function read(key: string) {
  try {
    return localStorage.getItem(key)
  } catch {
    return null
  }
}
function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value)
  } catch {
    /* private mode: ignore */
  }
}

export function PrefsProvider({ children }: { children: React.ReactNode }) {
  const [lang, setLangState] = React.useState<Lang>(() => {
    const saved = read("ruask-lang")
    if (saved === "zh" || saved === "en") return saved
    return navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en"
  })
  const [theme, setTheme] = React.useState<Theme>(() => {
    const saved = read("ruask-theme")
    if (saved === "light" || saved === "dark") return saved
    return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark"
  })

  React.useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark")
  }, [theme])

  React.useEffect(() => {
    document.documentElement.lang = lang === "zh" ? "zh-CN" : "en"
    document.title = lang === "zh" ? "ruask — Rust 编写的终端命令速查" : "ruask — a terminal command lookup, written in Rust"
  }, [lang])

  const value = React.useMemo<Prefs>(
    () => ({
      lang,
      t: copy[lang],
      setLang: (l) => {
        setLangState(l)
        write("ruask-lang", l)
      },
      theme,
      toggleTheme: () =>
        setTheme((prev) => {
          const next = prev === "dark" ? "light" : "dark"
          write("ruask-theme", next)
          return next
        }),
    }),
    [lang, theme]
  )

  return <PrefsContext.Provider value={value}>{children}</PrefsContext.Provider>
}

export function usePrefs() {
  const ctx = React.useContext(PrefsContext)
  if (!ctx) throw new Error("usePrefs must be used inside PrefsProvider")
  return ctx
}
