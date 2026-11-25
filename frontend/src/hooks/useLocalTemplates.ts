import { useCallback, useState } from 'react'
import type { LocalTemplate } from '../types'

const STORAGE_KEY = 'telecore-local-templates'

/**
 * Template pesan LOKAL perangkat (localStorage).
 * Backend v1 belum menyediakan endpoint template terpisah, jadi template
 * disimpan di perangkat dan TIDAK disinkronkan ke server. UI wajib
 * memberi label "template lokal perangkat".
 */
function loadTemplates(): LocalTemplate[] {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw) as LocalTemplate[]
    return Array.isArray(parsed) ? parsed : []
  } catch {
    return []
  }
}

export function useLocalTemplates() {
  const [templates, setTemplates] = useState<LocalTemplate[]>(loadTemplates)

  const persist = useCallback((next: LocalTemplate[]) => {
    setTemplates(next)
    try {
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify(next))
    } catch {
      // storage penuh — abaikan
    }
  }, [])

  const addTemplate = useCallback(
    (t: Omit<LocalTemplate, 'id' | 'created_at'>) => {
      const next: LocalTemplate = {
        ...t,
        id: `local-${Date.now()}`,
        created_at: new Date().toISOString(),
      }
      persist([...templates, next])
      return next
    },
    [templates, persist],
  )

  const removeTemplate = useCallback(
    (id: string) => persist(templates.filter((t) => t.id !== id)),
    [templates, persist],
  )

  return { templates, addTemplate, removeTemplate }
}
