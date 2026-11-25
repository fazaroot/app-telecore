import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { EmptyState, Badge } from '../components/ui'

describe('EmptyState', () => {
  it('menampilkan judul dan deskripsi', () => {
    render(<EmptyState title="Belum ada bot" description="Tambahkan bot pertama Anda." />)
    expect(screen.getByText('Belum ada bot')).toBeInTheDocument()
    expect(screen.getByText('Tambahkan bot pertama Anda.')).toBeInTheDocument()
  })
})

describe('Badge', () => {
  it('menampilkan isi badge', () => {
    render(<Badge tone="success">Terhubung</Badge>)
    expect(screen.getByText('Terhubung')).toBeInTheDocument()
  })
})
