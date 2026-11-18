import * as React from "react"

import { cn } from "@/lib/utils"

/*
 * Cards are drafting panels: flat, 1px rule, no shadow, and four
 * crosshair corner marks that grow and turn blueprint on hover (`group/card`).
 */
function CornerMarks({ className }: { className?: string }) {
  const base =
    "pointer-events-none absolute size-2.5 border-rule-strong transition-all duration-300 ease-draft group-hover/card:size-4 group-hover/card:border-blueprint"
  return (
    <span aria-hidden className={className}>
      <span className={cn(base, "-top-px -left-px border-t border-l")} />
      <span className={cn(base, "-top-px -right-px border-t border-r")} />
      <span className={cn(base, "-bottom-px -left-px border-b border-l")} />
      <span className={cn(base, "-right-px -bottom-px border-r border-b")} />
    </span>
  )
}

function Card({
  className,
  marks = true,
  children,
  ...props
}: React.ComponentProps<"div"> & { marks?: boolean }) {
  return (
    <div
      data-slot="card"
      className={cn(
        "group/card relative flex flex-col gap-4 border border-rule bg-card py-5 text-card-foreground",
        className
      )}
      {...props}
    >
      {marks && <CornerMarks />}
      {children}
    </div>
  )
}

function CardHeader({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-header"
      className={cn(
        "grid auto-rows-min items-start gap-1.5 px-5 has-data-[slot=card-action]:grid-cols-[1fr_auto]",
        className
      )}
      {...props}
    />
  )
}

function CardTitle({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-title"
      className={cn("text-[15px] leading-snug font-medium", className)}
      {...props}
    />
  )
}

function CardDescription({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-description"
      className={cn("text-sm leading-relaxed text-muted-foreground", className)}
      {...props}
    />
  )
}

function CardAction({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-action"
      className={cn("col-start-2 row-span-2 row-start-1 self-start justify-self-end", className)}
      {...props}
    />
  )
}

function CardContent({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="card-content" className={cn("px-5", className)} {...props} />
}

function CardFooter({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div data-slot="card-footer" className={cn("flex items-center px-5", className)} {...props} />
  )
}

export {
  Card,
  CardHeader,
  CardFooter,
  CardTitle,
  CardAction,
  CardDescription,
  CardContent,
  CornerMarks,
}
