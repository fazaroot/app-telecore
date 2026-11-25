import { apiRequest, asArray } from './client'
import type {
  ScheduledTask,
  SchedulerHistoryEntry,
  Workflow,
  WorkflowExecution,
} from '../types'

export async function fetchWorkflows(botId?: string | number): Promise<Workflow[]> {
  const data = await apiRequest<unknown>('/api/automation', {
    query: botId !== undefined ? { bot_id: botId } : undefined,
  })
  return asArray<Workflow>(data, ['workflows', 'items', 'data'])
}

export async function createWorkflow(payload: Partial<Workflow>): Promise<Workflow> {
  return apiRequest<Workflow>('/api/automation', { method: 'POST', body: payload })
}

export async function updateWorkflow(id: string | number, payload: Partial<Workflow>): Promise<Workflow> {
  return apiRequest<Workflow>(`/api/automation/${id}`, { method: 'PUT', body: payload })
}

export async function deleteWorkflow(id: string | number): Promise<void> {
  await apiRequest(`/api/automation/${id}`, { method: 'DELETE' })
}

export async function toggleWorkflow(id: string | number): Promise<Workflow> {
  return apiRequest<Workflow>(`/api/automation/${id}/toggle`, { method: 'POST' })
}

export async function fetchWorkflowExecutions(workflowId?: string | number): Promise<WorkflowExecution[]> {
  const path = workflowId !== undefined ? `/api/automation/${workflowId}/executions` : '/api/automation/executions'
  const data = await apiRequest<unknown>(path)
  return asArray<WorkflowExecution>(data, ['executions', 'items', 'data']).map((ex) => ({
    ...ex,
    started_at: ex.started_at ?? ex.created_at,
    error: ex.error ?? ex.error_message ?? null,
  }))
}

export async function fetchTasks(botId?: string | number): Promise<ScheduledTask[]> {
  const data = await apiRequest<unknown>('/api/scheduler', {
    query: botId !== undefined ? { bot_id: botId } : undefined,
  })
  return asArray<ScheduledTask>(data, ['tasks', 'items', 'data'])
}

export async function createTask(payload: Partial<ScheduledTask>): Promise<ScheduledTask> {
  return apiRequest<ScheduledTask>('/api/scheduler', { method: 'POST', body: payload })
}

export async function updateTask(id: string | number, payload: Partial<ScheduledTask>): Promise<ScheduledTask> {
  return apiRequest<ScheduledTask>(`/api/scheduler/${id}`, { method: 'PUT', body: payload })
}

export async function deleteTask(id: string | number): Promise<void> {
  await apiRequest(`/api/scheduler/${id}`, { method: 'DELETE' })
}

export async function pauseTask(id: string | number): Promise<ScheduledTask> {
  return apiRequest<ScheduledTask>(`/api/scheduler/${id}/pause`, { method: 'POST' })
}

export async function resumeTask(id: string | number): Promise<ScheduledTask> {
  return apiRequest<ScheduledTask>(`/api/scheduler/${id}/resume`, { method: 'POST' })
}

export async function fetchSchedulerHistory(taskId?: string | number): Promise<SchedulerHistoryEntry[]> {
  const path = taskId !== undefined ? `/api/scheduler/${taskId}/history` : '/api/scheduler/history'
  const data = await apiRequest<unknown>(path)
  // Backend mengembalikan ActivityLog — petakan ke bentuk riwayat task.
  return asArray<Record<string, unknown>>(data, ['history', 'items', 'data']).map((h) => {
    const details =
      typeof h.details === 'string'
        ? (safeJson(h.details) as Record<string, unknown>)
        : (h.details as Record<string, unknown> | undefined)
    const action = typeof h.action === 'string' ? h.action : undefined
    return {
      id: (h.id as string | number) ?? '',
      task_id: (h.entity_id ?? h.task_id) as string | number | undefined,
      status:
        action === 'scheduled_task_failed'
          ? 'failed'
          : action === 'scheduled_task_executed'
            ? 'success'
            : ((h.status as string | undefined) ?? action ?? '—'),
      run_at: (h.run_at ?? h.created_at) as string | undefined,
      error: (details?.error as string | undefined) ?? (h.error as string | undefined) ?? null,
      action,
      level: h.level as string | undefined,
      created_at: h.created_at as string | undefined,
      details,
    }
  })
}

function safeJson(s: string): unknown {
  try {
    return JSON.parse(s) as unknown
  } catch {
    return undefined
  }
}
