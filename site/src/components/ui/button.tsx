import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"
import { Slot } from "radix-ui"

import { cn } from "@/lib/utils"

/*
 * Blueprint Terminal buttons: mono labels, 4px corners, 1px rules.
 * Extra variants beyond shadcn's set:
 *   bracket  — "[ label ]" nav-style link; brackets spread on hover (from Atuin)
 *   terminal — hexagonal "$ install" chip in prompt green (from Atuin)
 */
const buttonVariants = cva(
  "group/button relative inline-flex shrink-0 cursor-pointer items-center justify-center gap-2 font-mono text-[13px] font-medium whitespace-nowrap transition-[color,background-color,border-color,transform,filter] duration-200 ease-draft outline-none select-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background active:translate-y-px disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
  {
    variants: {
      variant: {
        default:
          "rounded-md bg-primary text-primary-foreground hover:bg-[color-mix(in_oklab,var(--primary)_86%,var(--ink))]",
        outline:
          "rounded-md border border-rule-strong bg-transparent text-foreground hover:border-blueprint hover:bg-blueprint-soft",
        secondary:
          "rounded-md bg-secondary text-secondary-foreground hover:bg-[color-mix(in_oklab,var(--secondary)_80%,var(--ink))]",
        ghost: "rounded-md text-ink-muted hover:bg-paper-sunken hover:text-foreground",
        bracket:
          "text-ink-muted hover:text-foreground before:pr-1.5 before:text-ink-faint before:transition-transform before:duration-200 before:content-['['] after:pl-1.5 after:text-ink-faint after:transition-transform after:duration-200 after:content-[']'] hover:before:-translate-x-0.5 hover:after:translate-x-0.5 aria-[current=true]:text-foreground aria-[current=true]:before:text-blueprint aria-[current=true]:after:text-blueprint",
        terminal:
          "hex-clip bg-term-prompt text-[#0d2412] before:opacity-60 before:content-['$'] hover:brightness-110",
        destructive: "rounded-md bg-destructive text-white hover:bg-destructive/90",
        link: "text-blueprint underline-offset-4 hover:underline",
      },
      size: {
        default: "h-9 px-4 has-[>svg]:px-3",
        xs: "h-6 gap-1 px-2 text-xs [&_svg:not([class*='size-'])]:size-3",
        sm: "h-8 gap-1.5 px-3 text-xs has-[>svg]:px-2.5",
        lg: "h-11 px-5 text-sm has-[>svg]:px-4",
        icon: "size-9",
        "icon-xs": "size-6 [&_svg:not([class*='size-'])]:size-3",
        "icon-sm": "size-8",
        "icon-lg": "size-10",
      },
    },
    compoundVariants: [
      { variant: "bracket", className: "h-auto px-0 has-[>svg]:px-0" },
      { variant: "terminal", size: "default", className: "px-6" },
      { variant: "terminal", size: "lg", className: "px-7" },
    ],
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
)

/** Keyboard hint shown at the end of a button, Zed-style. */
function Kbd({ className, ...props }: React.ComponentProps<"kbd">) {
  return (
    <kbd
      data-slot="kbd"
      className={cn(
        "ml-0.5 inline-grid h-5 min-w-5 place-items-center rounded-sm border border-current/30 px-1 font-mono text-[11px] leading-none opacity-80",
        className
      )}
      {...props}
    />
  )
}

function Button({
  className,
  variant = "default",
  size = "default",
  asChild = false,
  ...props
}: React.ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & {
    asChild?: boolean
  }) {
  const Comp = asChild ? Slot.Root : "button"

  return (
    <Comp
      data-slot="button"
      data-variant={variant}
      data-size={size}
      className={cn(buttonVariants({ variant, size, className }))}
      {...props}
    />
  )
}

export { Button, Kbd, buttonVariants }
