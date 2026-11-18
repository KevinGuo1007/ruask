import * as React from "react"
import { useInView, useReducedMotion } from "motion/react"

import { CornerMarks } from "@/components/ui/card"

/*
 * Launch video inside a drafting frame with dimension annotations.
 * Muted autoplay only while half visible; a manual pause sticks.
 */
export function VideoFrame({ label }: { label: string }) {
  const video = React.useRef<HTMLVideoElement>(null)
  const inView = useInView(video, { amount: 0.5 })
  const reduce = useReducedMotion()
  const userPaused = React.useRef(false)
  const auto = React.useRef(false)

  React.useEffect(() => {
    const v = video.current
    if (!v || reduce) return
    if (inView && !userPaused.current && v.paused) {
      v.play().catch(() => {})
    } else if (!inView && !v.paused) {
      auto.current = true
      v.pause()
    }
  }, [inView, reduce])

  return (
    <div className="group/card relative pt-7 pl-0 md:pl-9">
      {/* horizontal dimension */}
      <div aria-hidden className="absolute top-0 right-0 left-0 flex items-center gap-2 font-mono text-[11px] text-ink-faint md:left-9">
        <span className="h-2.5 w-px bg-rule-strong" />
        <span className="h-px flex-1 bg-rule-strong" />
        <span>1920 px</span>
        <span className="h-px flex-1 bg-rule-strong" />
        <span className="h-2.5 w-px bg-rule-strong" />
      </div>
      {/* vertical dimension */}
      <div aria-hidden className="absolute top-7 bottom-0 left-0 hidden w-6 flex-col items-center gap-2 font-mono text-[11px] text-ink-faint md:flex">
        <span className="h-px w-2.5 bg-rule-strong" />
        <span className="w-px flex-1 bg-rule-strong" />
        <span className="[writing-mode:vertical-rl]">1080 px</span>
        <span className="w-px flex-1 bg-rule-strong" />
        <span className="h-px w-2.5 bg-rule-strong" />
      </div>
      <div className="relative border border-rule-strong bg-term-bg p-1.5">
        <CornerMarks />
        <video
          ref={video}
          className="block aspect-video w-full bg-black"
          controls
          muted
          loop
          playsInline
          preload="metadata"
          poster="assets/ruask-launch-poster.jpg"
          width={1920}
          height={1080}
          aria-label={label}
          onPause={() => {
            if (!auto.current && !video.current?.ended) userPaused.current = true
            auto.current = false
          }}
          onPlay={() => {
            userPaused.current = false
          }}
        >
          <source src="assets/ruask-launch.mp4" type="video/mp4" />
          <source src="assets/ruask-launch.webm" type="video/webm" />
        </video>
      </div>
    </div>
  )
}
