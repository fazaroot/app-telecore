import type { ReactNode } from 'react'
import { Loader2 } from 'lucide-react'

export function Card({ children, className = '' }: { children: ReactNode; className?: string }) {
  return (
    <div className={`rounded-2xl bg-surface p-4 shadow-sm dark:bg-darksurface ${className}`}>{children}</div>
  )
}

export function LoadingSpinner({ label = 'Memuat…' }: { label?: string }) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 py-10 text-ink/60 dark:text-darktext/60">
      <Loader2 className="h-7 w-7 animate-spin text-primary" />
      <p className="text-sm">{label}</p>
    </div>
  )
}

export function EmptyState({
  icon,
  title,
  description,
  action,
}: {
  icon?: ReactNode
  title: string
  description?: string
  action?: ReactNode
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 rounded-2xl border border-dashed border-ink/20 px-4 py-10 text-center dark:border-darktext/20">
      {icon}
      <p className="font-semibold text-ink dark:text-darktext">{title}</p>
      {description && <p className="max-w-sm text-sm text-ink/60 dark:text-darktext/60">{description}</p>}
      {action}
    </div>
  )
}

export function ErrorState({ message, onRetry }: { message: string; onRetry?: () => void }) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 rounded-2xl bg-danger/10 px-4 py-8 text-center">
      <p className="font-semibold text-danger">Gagal memuat data</p>
      <p className="max-w-sm text-sm text-ink/70 dark:text-darktext/70">{message}</p>
      {onRetry && (
        <button
          type="button"
          onClick={onRetry}
          className="rounded-full bg-primary px-4 py-2 text-sm font-semibold text-white"
        >
          Coba lagi
        </button>
      )}
    </div>
  )
}

export function PageHeader({ title, subtitle, action }: { title: string; subtitle?: string; action?: ReactNode }) {
  return (
    <div className="mb-4 flex items-start justify-between gap-3">
      <div>
        <h1 className="text-xl font-bold text-ink dark:text-darktext">{title}</h1>
        {subtitle && <p className="mt-0.5 text-sm text-ink/60 dark:text-darktext/60">{subtitle}</p>}
      </div>
      {action}
    </div>
  )
}

export function Badge({
  children,
  tone = 'default',
}: {
  children: ReactNode
  tone?: 'default' | 'success' | 'danger' | 'warning' | 'info'
}) {
  const tones: Record<string, string> = {
    default: 'bg-ink/10 text-ink dark:bg-darktext/10 dark:text-darktext',
    success: 'bg-success/15 text-success',
    danger: 'bg-danger/15 text-danger',
    warning: 'bg-amber-500/15 text-amber-600',
    info: 'bg-primary/15 text-primary',
  }
  return (
    <span className={`inline-flex items-center rounded-full px-2 py-0.5 text-xs font-semibold ${tones[tone]}`}>
      {children}
    </span>
  )
}
