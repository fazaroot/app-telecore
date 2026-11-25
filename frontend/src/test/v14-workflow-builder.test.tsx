import { useState } from 'react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { Workflow } from '../types'
import { WorkflowBuilder } from '../components/WorkflowBuilder'
import {
  createActionNode,
  createConditionNode,
  createTriggerNode,
  moveActionNode,
  nodesToWorkflowPayload,
  validateBuilder,
  workflowToNodes,
  type BuilderState,
} from '../components/workflowNodes'

/**
 * v1.4 Visual Automation Builder:
 * - Node contoh Astr (Trigger pesan masuk → Condition kata kunci →
 *   send_message → save_activity → notify_admin) harus menjadi payload benar.
 * - Workflow lama (send_template/add_label/save_note/delay, kondisi OR,
 *   delay_seconds) harus termuat tanpa kehilangan data.
 */

vi.mock('../api/bots', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/bots')>()
  return { ...actual, fetchBots: vi.fn(async () => [{ id: 'b1', name: 'Bot testing' }]) }
})

vi.mock('../api/automation', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/automation')>()
  return {
    ...actual,
    fetchWorkflows: vi.fn(async () => []),
    createWorkflow: vi.fn(async (payload: unknown) => ({ id: 'w9', ...(payload as object) })),
  }
})

import { createWorkflow } from '../api/automation'
import { AutomationPage } from '../pages/AutomationPage'

const createWorkflowMock = vi.mocked(createWorkflow)

function astrExampleState(): BuilderState {
  return {
    trigger: createTriggerNode('message_received'),
    conditions: [createConditionNode({ field: 'text', operator: 'contains', value: 'harga' })],
    actions: [
      createActionNode('send_message', { text: 'Halo {{first_name}}, harga produk kami mulai Rp10.000.' }),
      createActionNode('save_activity', { text: 'Pengguna menanyakan harga' }),
      createActionNode('notify_admin', { text: 'Ada yang menanyakan harga: {{text}}', admin_chat_id: '123456789' }),
    ],
    conditionLogic: 'AND',
  }
}

describe('mapping node contoh Astr menjadi payload (v1.4)', () => {
  it('Trigger pesan masuk → Condition kata kunci → send_message → save_activity → notify_admin', () => {
    const payload = nodesToWorkflowPayload(astrExampleState(), {
      botId: 'b1',
      name: 'Balas harga',
      isEnabled: true,
    })
    expect(payload.bot_id).toBe('b1')
    expect(payload.name).toBe('Balas harga')
    expect(payload.trigger_type).toBe('message_received')
    expect(payload.conditions).toEqual([{ field: 'text', operator: 'contains', value: 'harga' }])
    expect(payload.actions).toEqual([
      { type: 'send_message', text: 'Halo {{first_name}}, harga produk kami mulai Rp10.000.' },
      { type: 'save_activity', text: 'Pengguna menanyakan harga' },
      { type: 'notify_admin', text: 'Ada yang menanyakan harga: {{text}}', admin_chat_id: 123456789 },
    ])
    expect(payload.is_enabled).toBe(true)
    expect(validateBuilder(astrExampleState(), { botId: 'b1', name: 'Balas harga' })).toEqual([])
  })
})

describe('workflow lama termuat tanpa kehilangan data (v1.4)', () => {
  it('send_template/add_label/save_note/delay, kondisi OR, dan delay_seconds dipertahankan', () => {
    const legacy: Workflow = {
      id: 'w1',
      bot_id: 'b1',
      name: 'Workflow lama',
      trigger_type: 'keyword',
      trigger_config: { keywords: ['promo', 'diskon'] },
      conditions: {
        logic: 'OR',
        rules: [
          { field: 'text', operator: 'contains', value: 'promo' },
          { field: 'chat_id', operator: 'eq', value: 555 },
        ],
      },
      actions: [
        { type: 'send_template', template_id: 'tpl-1', custom_key: 'tetap' },
        { type: 'add_label', label: 'promo' },
        { type: 'save_note', note: 'Catatan lama' },
        { type: 'delay', seconds: 5 },
        { type: 'send_message', text: 'Halo', delay_seconds: 3 },
      ],
      is_enabled: true,
    }

    const nodes = workflowToNodes(legacy)
    expect(nodes.trigger.triggerType).toBe('keyword')
    expect(nodes.conditionLogic).toBe('OR')
    expect(nodes.actions.map((a) => a.action.type)).toEqual([
      'send_template',
      'add_label',
      'save_note',
      'delay',
      'send_message',
    ])

    const payload = nodesToWorkflowPayload(nodes, { botId: 'b1', name: 'Workflow lama', isEnabled: true })
    expect(payload.trigger_config.keywords).toEqual(['promo', 'diskon'])
    expect(payload.conditions).toEqual({
      logic: 'OR',
      rules: [
        { field: 'text', operator: 'contains', value: 'promo' },
        { field: 'chat_id', operator: 'equals', value: 555 },
      ],
    })
    expect(payload.actions[0]).toMatchObject({ type: 'send_template', template_id: 'tpl-1', custom_key: 'tetap' })
    expect(payload.actions[1]).toMatchObject({ type: 'add_label', label: 'promo' })
    expect(payload.actions[2]).toMatchObject({ type: 'save_note', note: 'Catatan lama' })
    expect(payload.actions[3]).toMatchObject({ type: 'delay', seconds: 5 })
    expect(payload.actions[4]).toMatchObject({ type: 'send_message', text: 'Halo', delay_seconds: 3 })
  })
})

describe('validasi builder (v1.4)', () => {
  it('menolak bot/nama kosong, condition kosong, dan teks balasan kosong', () => {
    const state: BuilderState = {
      trigger: createTriggerNode('message_received'),
      conditions: [createConditionNode({ field: 'text', operator: 'contains', value: '' })],
      actions: [createActionNode('send_message', { text: '' })],
      conditionLogic: 'AND',
    }
    const errors = validateBuilder(state, { botId: '', name: '' })
    expect(errors.join(' ')).toContain('Pilih bot')
    expect(errors.join(' ')).toContain('Nama workflow')
    expect(errors.join(' ')).toContain('kata kunci tidak boleh kosong')
    expect(errors.join(' ')).toContain('teks balasan tidak boleh kosong')
  })

  it('menolak workflow tanpa action dan notifikasi tanpa teks', () => {
    const noActions: BuilderState = {
      trigger: createTriggerNode('message_received'),
      conditions: [],
      actions: [],
      conditionLogic: 'AND',
    }
    expect(validateBuilder(noActions, { botId: 'b1', name: 'X' }).join(' ')).toContain('minimal satu action')

    const badNotify: BuilderState = {
      trigger: createTriggerNode('message_received'),
      conditions: [],
      actions: [createActionNode('notify_admin', { text: '', admin_chat_id: 'bukan-angka' })],
      conditionLogic: 'AND',
    }
    const errors = validateBuilder(badNotify, { botId: 'b1', name: 'X' }).join(' ')
    expect(errors).toContain('teks notifikasi tidak boleh kosong')
    expect(errors).toContain('admin_chat_id harus berupa angka')

    const badActivity: BuilderState = {
      trigger: createTriggerNode('message_received'),
      conditions: [],
      actions: [createActionNode('save_activity', { text: '' })],
      conditionLogic: 'AND',
    }
    expect(validateBuilder(badActivity, { botId: 'b1', name: 'X' }).join(' ')).toContain(
      'teks aktivitas tidak boleh kosong',
    )

    const legacyUnsupported: BuilderState = {
      trigger: createTriggerNode('message_received'),
      conditions: [],
      actions: [createActionNode('reply', { text: 'Halo' })],
      conditionLogic: 'AND',
    }
    expect(validateBuilder(legacyUnsupported, { botId: 'b1', name: 'X' }).join(' ')).toContain(
      'tidak didukung backend',
    )
  })

  it('moveActionNode menukar urutan action', () => {
    const moved = moveActionNode(astrExampleState(), 0, 1)
    expect(moved.actions.map((a) => a.action.type)).toEqual(['save_activity', 'send_message', 'notify_admin'])
  })
})

function BuilderHarness() {
  const [state, setState] = useState<BuilderState>(() => astrExampleState())
  return <WorkflowBuilder state={state} onChange={setState} />
}

describe('kanvas builder visual (v1.4)', () => {
  it('klik node membuka Properties, tambah action, dan urutkan action', () => {
    render(<BuilderHarness />)

    expect(screen.getByLabelText(/Trigger:/)).toBeInTheDocument()
    expect(screen.getByLabelText('Action 1: Kirim balasan')).toBeInTheDocument()
    expect(screen.getByLabelText('Action 2: Simpan aktivitas')).toBeInTheDocument()
    expect(screen.getByLabelText('Action 3: Kirim notifikasi admin')).toBeInTheDocument()

    fireEvent.click(screen.getByLabelText('Action 1: Kirim balasan'))
    expect(screen.getByText(/Properties — Action 1/)).toBeInTheDocument()
    expect(screen.getByDisplayValue(/harga produk kami/)).toBeInTheDocument()

    fireEvent.click(screen.getByText('Tambah Action'))
    expect(screen.getByLabelText('Action 4: Kirim balasan')).toBeInTheDocument()

    fireEvent.click(screen.getAllByLabelText('Pindah action ke bawah')[0])
    expect(screen.getByLabelText('Action 1: Simpan aktivitas')).toBeInTheDocument()
  })
})

describe('AutomationPage memakai builder visual (v1.4)', () => {
  it('validasi tampil sebelum simpan, lalu workflow tersimpan dengan payload benar', async () => {
    createWorkflowMock.mockClear()
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
    })
    render(
      <QueryClientProvider client={queryClient}>
        <AutomationPage />
      </QueryClientProvider>,
    )

    expect(await screen.findByText('Belum ada workflow', {}, { timeout: 10_000 })).toBeInTheDocument()
    fireEvent.click(screen.getByText('Buat Workflow'))
    expect(await screen.findByText('Workflow Baru', {}, { timeout: 10_000 })).toBeInTheDocument()

    fireEvent.click(screen.getByText('Simpan Workflow'))
    expect(await screen.findByText('Pilih bot terlebih dahulu.')).toBeInTheDocument()
    expect(screen.getByText('Nama workflow wajib diisi.')).toBeInTheDocument()
    expect(createWorkflowMock).not.toHaveBeenCalled()

    fireEvent.change(screen.getByLabelText('Bot'), { target: { value: 'b1' } })
    fireEvent.change(screen.getByPlaceholderText(/Balas kata/), { target: { value: 'Balas harga' } })
    fireEvent.click(screen.getByLabelText('Action 1: Kirim balasan'))
    fireEvent.change(screen.getByPlaceholderText(/Halo \{\{first_name\}\}/), {
      target: { value: 'Halo {{first_name}}!' },
    })
    fireEvent.click(screen.getByText('Simpan Workflow'))

    await waitFor(() => expect(createWorkflowMock).toHaveBeenCalledTimes(1))
    expect(createWorkflowMock).toHaveBeenCalledWith(
      expect.objectContaining({
        bot_id: 'b1',
        name: 'Balas harga',
        trigger_type: 'message_received',
        conditions: [],
        actions: [{ type: 'send_message', text: 'Halo {{first_name}}!' }],
        is_enabled: true,
      }),
    )
  }, 20_000)
})
