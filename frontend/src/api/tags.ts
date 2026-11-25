import { apiRequest, asArray } from './client'
import type { TimelineItem, UserNote, UserTag } from '../types'

export async function fetchTags(): Promise<UserTag[]> {
  const data = await apiRequest<unknown>('/api/tags')
  return asArray<UserTag>(data, ['tags', 'items', 'data'])
}

export async function createTag(payload: { name: string; color?: string }): Promise<UserTag> {
  return apiRequest<UserTag>('/api/tags', { method: 'POST', body: payload })
}

export async function updateTag(
  id: string | number,
  payload: Partial<{ name: string; color: string }>,
): Promise<UserTag> {
  return apiRequest<UserTag>(`/api/tags/${id}`, { method: 'PUT', body: payload })
}

export async function deleteTag(id: string | number): Promise<void> {
  await apiRequest(`/api/tags/${id}`, { method: 'DELETE' })
}

export async function fetchUserTags(userId: string | number): Promise<UserTag[]> {
  const data = await apiRequest<unknown>(`/api/users/${userId}/tags`)
  return asArray<UserTag>(data, ['tags', 'items', 'data'])
}

export async function attachUserTag(userId: string | number, tagId: string | number): Promise<UserTag> {
  return apiRequest<UserTag>(`/api/users/${userId}/tags`, { method: 'POST', body: { tag_id: tagId } })
}

export async function detachUserTag(userId: string | number, tagId: string | number): Promise<void> {
  await apiRequest(`/api/users/${userId}/tags/${tagId}`, { method: 'DELETE' })
}

export async function fetchUserNotes(userId: string | number): Promise<UserNote[]> {
  const data = await apiRequest<unknown>(`/api/users/${userId}/notes`)
  return asArray<UserNote>(data, ['notes', 'items', 'data'])
}

export async function addUserNote(userId: string | number, note: string): Promise<UserNote> {
  return apiRequest<UserNote>(`/api/users/${userId}/notes`, { method: 'POST', body: { note } })
}

export async function deleteUserNote(userId: string | number, noteId: string | number): Promise<void> {
  await apiRequest(`/api/users/${userId}/notes/${noteId}`, { method: 'DELETE' })
}

export async function fetchUserTimeline(userId: string | number): Promise<TimelineItem[]> {
  const data = await apiRequest<unknown>(`/api/users/${userId}/timeline`)
  return asArray<TimelineItem>(data, ['items', 'timeline', 'data'])
}

/** Warna default untuk tag baru (dipakai bergiliran). */
export const TAG_COLORS = ['#526FE5', '#16A34A', '#DC2626', '#D97706', '#7C3AED', '#0891B2', '#DB2777']
