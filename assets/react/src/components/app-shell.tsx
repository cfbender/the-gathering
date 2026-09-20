import { Link } from "@tanstack/react-router"
import type { ComponentProps, ReactNode } from "react"
import { cn } from "@/lib/cn"

/** Card-framed page heading shared by every top-level route (ported from manavault). */
export function PageHeader({
  title,
  bottomActions,
  description,
  actions,
  eyebrow,
  children,
}: {
  title: ReactNode
  bottomActions?: ReactNode
  description?: ReactNode
  actions?: ReactNode
  eyebrow?: ReactNode
  children?: ReactNode
}) {
  return (
    <section className="card border-base-300 bg-base-200 relative border">
      <div className="card-body gap-5 p-6 sm:p-8">
        <div className="flex flex-col gap-5 sm:flex-row sm:items-start sm:justify-between">
          <div className="min-w-0 flex-1">
            {eyebrow ? (
              <div className="badge badge-primary badge-outline mb-4 uppercase">{eyebrow}</div>
            ) : null}
            <h1 className="text-4xl font-black tracking-normal sm:text-5xl">{title}</h1>
            {description ? (
              <p className="text-base-content/70 mt-4 max-w-4xl text-lg">{description}</p>
            ) : null}
            {children}
            {bottomActions ? (
              <div className="mt-5 flex flex-wrap items-center justify-end gap-2">
                {bottomActions}
              </div>
            ) : null}
          </div>
          {actions ? (
            <div className="flex shrink-0 flex-wrap items-center gap-2">{actions}</div>
          ) : null}
        </div>
      </div>
    </section>
  )
}

export function PageSection({
  title,
  count,
  children,
  className,
  ...props
}: {
  title?: string
  count?: ReactNode
  children: ReactNode
} & Omit<ComponentProps<"section">, "title">) {
  return (
    <section className={cn("space-y-3", className)} {...props}>
      {title || count ? (
        <div className="flex items-center justify-between gap-3">
          {title ? <h2 className="text-2xl font-black tracking-normal">{title}</h2> : <span />}
          {count ? (
            <span className="badge bg-base-200 border-transparent text-sm">{count}</span>
          ) : null}
        </div>
      ) : null}
      {children}
    </section>
  )
}

export function ActionCard({
  to,
  icon,
  badge,
  badgeTone = "primary",
  title,
  description,
}: {
  to: string
  icon: ReactNode
  badge: ReactNode
  badgeTone?: "primary" | "secondary" | "accent"
  title: string
  description: string
}) {
  const badgeClass = {
    primary: "badge-primary",
    secondary: "badge-secondary",
    accent: "badge-accent",
  }[badgeTone]

  return (
    <Link
      to={to}
      className="card group border-base-300 bg-base-100 hover:border-primary/40 h-full border shadow-sm transition-all hover:-translate-y-1 hover:shadow-xl"
    >
      <div className="card-body min-h-64 justify-between p-6">
        <div className="flex items-start justify-between gap-4">
          <div className="text-5xl leading-none">{icon}</div>
          <span className={cn("badge badge-lg badge-outline", badgeClass)}>{badge}</span>
        </div>
        <div>
          <h2 className="text-3xl font-black tracking-normal">{title}</h2>
          <p className="text-base-content/70 mt-3 text-lg leading-8">{description}</p>
        </div>
      </div>
    </Link>
  )
}

export function EmptyPanel({
  title,
  description,
  icon,
  action,
}: {
  title: string
  description?: string
  icon?: ReactNode
  action?: ReactNode
}) {
  return (
    <div className="card border-base-300 bg-base-100 border p-8 text-center">
      <div className="flex min-w-0 flex-col items-center gap-3">
        {icon ? <div className="text-primary">{icon}</div> : null}
        <div>
          <h2 className="text-xl font-black">{title}</h2>
          {description ? <p className="text-base-content/70 mt-2">{description}</p> : null}
        </div>
        {action ? <div className="mt-2">{action}</div> : null}
      </div>
    </div>
  )
}
