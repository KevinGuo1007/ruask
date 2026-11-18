import * as React from "react"
import { useInView, useReducedMotion } from "motion/react"

/*
 * Ghostty-style animated ASCII field: the "ruask" wordmark is sampled from an
 * offscreen canvas into a character grid, then a slow diagonal wave ripples
 * through the density ramp. Rendered as plain text, paused off-screen, static
 * under prefers-reduced-motion.
 */
const RAMP = " .:-=+*#%@"

function sampleMask(cols: number, rows: number, text: string) {
  const scale = 4
  const canvas = document.createElement("canvas")
  canvas.width = cols * scale
  canvas.height = rows * scale * 2
  const ctx = canvas.getContext("2d")
  if (!ctx) return new Float32Array(cols * rows)
  ctx.fillStyle = "#000"
  ctx.fillRect(0, 0, canvas.width, canvas.height)
  ctx.fillStyle = "#fff"
  ctx.textAlign = "center"
  ctx.textBaseline = "middle"
  let size = canvas.height * 0.95
  ctx.font = `600 ${size}px "IBM Plex Mono", Menlo, monospace`
  const width = ctx.measureText(text).width
  if (width > canvas.width * 0.92) {
    size *= (canvas.width * 0.92) / width
    ctx.font = `600 ${size}px "IBM Plex Mono", Menlo, monospace`
  }
  ctx.fillText(text, canvas.width / 2, canvas.height / 2 + size * 0.04)
  const data = ctx.getImageData(0, 0, canvas.width, canvas.height).data
  const mask = new Float32Array(cols * rows)
  const cw = canvas.width / cols
  const ch = canvas.height / rows
  for (let y = 0; y < rows; y++) {
    for (let x = 0; x < cols; x++) {
      let sum = 0
      let n = 0
      for (let sy = 0; sy < ch; sy += 2) {
        for (let sx = 0; sx < cw; sx += 2) {
          const px = Math.floor(x * cw + sx)
          const py = Math.floor(y * ch + sy)
          sum += data[(py * canvas.width + px) * 4]
          n++
        }
      }
      mask[y * cols + x] = sum / n / 255
    }
  }
  return mask
}

export function AsciiWordmark({ text = "ruask" }: { text?: string }) {
  const wrap = React.useRef<HTMLDivElement>(null)
  const pre = React.useRef<HTMLPreElement>(null)
  const inView = useInView(wrap)
  const reduce = useReducedMotion()
  const [grid, setGrid] = React.useState<{ cols: number; rows: number; mask: Float32Array } | null>(null)

  React.useEffect(() => {
    const el = wrap.current
    if (!el) return
    const build = () => {
      // measure the real glyph width of the <pre> so the grid always fits
      const probe = document.createElement("span")
      probe.textContent = "MMMMMMMMMM"
      pre.current?.appendChild(probe)
      const charW = probe.getBoundingClientRect().width / 10 || 7
      probe.remove()
      const cols = Math.max(40, Math.min(132, Math.floor(el.clientWidth / charW) - 2))
      const rows = Math.round(cols / 5)
      setGrid({ cols, rows, mask: sampleMask(cols, rows, text) })
    }
    // wait for the self-hosted mono font so the sampled glyphs match
    document.fonts.load('600 40px "IBM Plex Mono"').finally(build)
    const ro = new ResizeObserver(build)
    ro.observe(el)
    return () => ro.disconnect()
  }, [text])

  React.useEffect(() => {
    if (!grid || !pre.current) return
    const { cols, rows, mask } = grid
    const node = pre.current
    const frame = (time: number) => {
      const t = time / 1000
      let html = ""
      for (let y = 0; y < rows; y++) {
        let line = ""
        let inside = false
        for (let x = 0; x < cols; x++) {
          const m = mask[y * cols + x]
          const wave = Math.sin(x * 0.18 + y * 0.42 - t * 1.6) * 0.5 + 0.5
          const drift = Math.sin(x * 0.05 - y * 0.3 + t * 0.7) * 0.5 + 0.5
          const isIn = m > 0.35
          let v = isIn ? 0.55 + 0.45 * wave : wave * drift * 0.32
          if (!isIn && v < 0.12) v = 0
          const c = RAMP[Math.min(RAMP.length - 1, Math.floor(v * RAMP.length))]
          if (isIn !== inside) {
            line += isIn ? '<b>' : "</b>"
            inside = isIn
          }
          line += c
        }
        if (inside) line += "</b>"
        html += line + "\n"
      }
      node.innerHTML = html
    }
    if (reduce || !inView) {
      frame(0)
      return
    }
    let raf = 0
    let last = 0
    const loop = (time: number) => {
      if (time - last > 66) {
        frame(time)
        last = time
      }
      raf = requestAnimationFrame(loop)
    }
    raf = requestAnimationFrame(loop)
    return () => cancelAnimationFrame(raf)
  }, [grid, inView, reduce])

  return (
    <div ref={wrap} className="w-full overflow-hidden" aria-hidden>
      <pre
        ref={pre}
        className="mx-auto w-fit font-mono text-[7px] leading-[1.15] text-rule-strong select-none sm:text-[10px] lg:text-[12px] [&_b]:font-normal [&_b]:text-blueprint"
      />
    </div>
  )
}
