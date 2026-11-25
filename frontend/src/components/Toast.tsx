import { AnimatePresence, motion } from 'framer-motion'
import { AlertCircle, CheckCircle2, Info } from 'lucide-react'
import { useUiStore } from '../stores/settings'

export function Toasts() {
  const toasts = useUiStore((s) => s.toasts)
  const dismiss = useUiStore((s) => s.dismissToast)
  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-24 z-[60] flex flex-col items-center gap-2 px-4">
      <AnimatePresence>
        {toasts.map((t) => (
          <motion.button
            key={t.id}
            type="button"
            onClick={() => dismiss(t.id)}
            className="pointer-events-auto flex w-full max-w-md items-start gap-2 rounded-xl bg-ink px-4 py-3 text-left text-sm text-white shadow-lg dark:bg-darktext dark:text-ink"
            initial={{ opacity: 0, y: 16 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 16 }}
          >
            {t.kind === 'success' ? (
              <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-success" />
            ) : t.kind === 'error' ? (
              <AlertCircle className="mt-0.5 h-4 w-4 shrink-0 text-danger" />
            ) : (
              <Info className="mt-0.5 h-4 w-4 shrink-0 text-secondary" />
            )}
            <span>{t.message}</span>
          </motion.button>
        ))}
      </AnimatePresence>
    </div>
  )
}
