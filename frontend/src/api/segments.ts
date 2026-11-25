import { apiRequest, asArray } from './client'
import { fetchBroadcast } from './messages'
import type { Segment, SegmentCriteria, SegmentFollowUpResult, SegmentMembersPage } from '../types'

export function emptyCriteria(): SegmentCriteria {
  return {}
}

/** Ringkasan kriteria yang mudah dibaca untuk daftar segmen. */
export function criteriaSummary(criteria: SegmentCriteria | undefined, tagName?: (id: string | number) => string): string {
  if (!criteria) return 'Semua pengguna'
  const parts: string[] = []
  if (criteria.any_tags && criteria.any_tags.length > 0) {
    parts.push(`tag: ${criteria.any_tags.map((t) => tagName?.(t) ?? String(t)).join(', ')}`)
  }
  if (criteria.has_ordered) parts.push('pernah order')
  if (criteria.last_active_within_days !== undefined) {
    parts.push(`aktif ≤ ${criteria.last_active_within_days} hari terakhir`)
  }
  if (criteria.last_active_over_days_ago !== undefined) {
    parts.push(`tidak aktif > ${criteria.last_active_over_days_ago} hari`)
  }
  if (criteria.min_messages !== undefined) parts.push(`min. ${criteria.min_messages} pesan`)
  return parts.length > 0 ? parts.join(' · ') : 'Semua pengguna'
}

export async function fetchSegments(): Promise<Segment[]> {
  const data = await apiRequest<unknown>('/api/segments')
  return asArray<Segment>(data, ['segments', 'items', 'data'])
}

export async function createSegment(payload: { name: string; criteria: SegmentCriteria }): Promise<Segment> {
  return apiRequest<Segment>('/api/segments', { method: 'POST', body: payload })
}

export async function fetchSegment(id: string | number): Promise<Segment> {
  return apiRequest<Segment>(`/api/segments/${id}`)
}

export async function updateSegment(
  id: string | number,
  payload: Partial<{ name: string; criteria: SegmentCriteria }>,
): Promise<Segment> {
  return apiRequest<Segment>(`/api/segments/${id}`, { method: 'PUT', body: payload })
}

export async function deleteSegment(id: string | number): Promise<void> {
  await apiRequest(`/api/segments/${id}`, { method: 'DELETE' })
}

export async function previewSegment(criteria: SegmentCriteria): Promise<{ count: number }> {
  return apiRequest<{ count: number }>('/api/segments/preview', { method: 'POST', body: { criteria } })
}

export async function fetchSegmentMembers(
  id: string | number,
  page = 1,
  perPage = 50,
): Promise<SegmentMembersPage> {
  const data = await apiRequest<unknown>(`/api/segments/${id}/members`, {
    query: { page, per_page: perPage },
  })
  if (data && typeof data === 'object' && Array.isArray((data as Record<string, unknown>).items)) {
    const d = data as unknown as SegmentMembersPage
    return { items: d.items ?? [], total: d.total ?? d.items.length, page: d.page ?? page, per_page: d.per_page ?? perPage }
  }
  const items = asArray(data as unknown, ['items', 'members', 'data']) as SegmentMembersPage['items']
  return { items, total: items.length, page, per_page: perPage }
}

export async function sendSegmentFollowUp(
  id: string | number,
  payload: { message: string; parse_mode?: string },
): Promise<SegmentFollowUpResult> {
  return apiRequest<SegmentFollowUpResult>(`/api/segments/${id}/follow-up`, { method: 'POST', body: payload })
}

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms))

/**
 * Kirim follow-up lalu TUNGGU sampai selesai: respons POST awal backend
 * hanya snapshot awal (sent/failed 0, status 'running') karena pengiriman
 * berjalan di background via broadcast v1.1. Menampilkan angka snapshot itu
 * sebagai hasil akhir akan menyesatkan ("terkirim 0 dari N"), jadi fungsi
 * ini melakukan polling GET /api/messages/broadcast/:id untuk setiap
 * broadcast_id sampai status 'done' (atau batas waktu), lalu mengembalikan
 * angka final yang diagregasi dari semua broadcast (satu per bot).
 * Bila tidak ada broadcast (total 0 / status 'done'), langsung kembali.
 */
export async function sendSegmentFollowUpAndWait(
  id: string | number,
  payload: { message: string; parse_mode?: string },
  opts: { pollIntervalMs?: number; maxAttempts?: number } = {},
): Promise<SegmentFollowUpResult> {
  const pollIntervalMs = opts.pollIntervalMs ?? 1000
  const maxAttempts = opts.maxAttempts ?? 120
  const initial = await sendSegmentFollowUp(id, payload)
  const ids = initial.broadcast_ids ?? (initial.broadcast_id !== undefined && initial.broadcast_id !== null ? [initial.broadcast_id] : [])
  if (initial.status !== 'running' || ids.length === 0) {
    return { ...initial, status: initial.status ?? 'done' }
  }
  let sent = initial.sent
  let failed = initial.failed
  let total = initial.total
  let status = 'running'
  for (let attempt = 0; attempt < maxAttempts; attempt++) {
    await sleep(pollIntervalMs)
    let allDone = true
    let aggSent = 0
    let aggFailed = 0
    let aggTotal = 0
    for (const bid of ids) {
      let st
      try {
        st = await fetchBroadcast(bid)
      } catch {
        // Broadcast hilang (mis. server restart): pakai snapshot terakhir.
        allDone = true
        aggSent = sent
        aggFailed = failed
        aggTotal = total
        break
      }
      aggSent += st.sent ?? 0
      aggFailed += st.failed ?? 0
      aggTotal += st.total ?? 0
      if (st.status !== 'done') allDone = false
    }
    sent = aggSent
    failed = aggFailed
    total = aggTotal
    if (allDone) {
      status = 'done'
      break
    }
  }
  return { ...initial, sent, failed, total, status }
}
