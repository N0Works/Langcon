"use client"

import * as React from "react"

import { cn } from "@/lib/utils"

type ScrollAreaProps = React.HTMLAttributes<HTMLDivElement>

function ScrollArea({ className, children, ...props }: ScrollAreaProps) {
  return (
    <div
      data-slot="scroll-area"
      className={cn("relative", className)}
      {...props}
    >
      <div
        data-slot="scroll-area-viewport"
        className="focus-visible:ring-ring/50 size-full overflow-auto rounded-[inherit] transition-[color,box-shadow] outline-none focus-visible:ring-[3px] focus-visible:outline-1"
      >
        {children}
      </div>
    </div>
  )
}

function ScrollBar() {
  return null
}

export { ScrollArea, ScrollBar }
