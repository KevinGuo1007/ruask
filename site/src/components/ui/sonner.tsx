import type * as React from "react"
import { CheckIcon } from "lucide-react"
import { Toaster as Sonner, type ToasterProps } from "sonner"

/* Toasts read like a one-line terminal status message. */
const Toaster = ({ ...props }: ToasterProps) => {
  return (
    <Sonner
      className="toaster group"
      icons={{ success: <CheckIcon className="size-3.5 text-term-prompt" /> }}
      toastOptions={{
        classNames: {
          toast:
            "!gap-2 !rounded-sm !border !border-rule-strong !bg-term-bg !py-2.5 !font-mono !text-[12px] !text-term-fg !shadow-none",
        },
      }}
      style={
        {
          "--normal-bg": "var(--term-bg)",
          "--normal-text": "var(--term-fg)",
          "--normal-border": "var(--rule-strong)",
          "--border-radius": "var(--radius)",
        } as React.CSSProperties
      }
      {...props}
    />
  )
}

export { Toaster }
