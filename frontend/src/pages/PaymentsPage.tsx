import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { CreditCard, Plus, Send, Star, Trash2 } from 'lucide-react'
import {
  createPaymentProduct,
  deletePaymentProduct,
  fetchPaymentOrders,
  fetchPaymentProducts,
  formatPrice,
  productPrice,
  productTitle,
  sendPaymentInvoice,
  updatePaymentProduct,
  type PaymentProductPayload,
} from '../api/payments'
import { fetchBots } from '../api/bots'
import { useUiStore } from '../stores/settings'
import { Badge, Card, EmptyState, ErrorState, LoadingSpinner, PageHeader } from '../components/ui'
import { ConfirmDialog, Modal } from '../components/Modal'
import type { PaymentOrder, PaymentProduct } from '../types'

const inputCls =
  'w-full rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg'

function orderStatusBadge(status?: string) {
  switch ((status ?? '').toLowerCase()) {
    case 'paid':
      return <Badge tone="success">Dibayar</Badge>
    case 'pending':
      return <Badge tone="warning">Menunggu</Badge>
    case 'failed':
      return <Badge tone="danger">Gagal</Badge>
    case 'cancelled':
      return <Badge>Dibatalkan</Badge>
    default:
      return <Badge>{status ?? '—'}</Badge>
  }
}

export function PaymentsPage() {
  const queryClient = useQueryClient()
  const pushToast = useUiStore((s) => s.pushToast)
  const [showForm, setShowForm] = useState(false)
  const [editing, setEditing] = useState<PaymentProduct | null>(null)
  const [deleting, setDeleting] = useState<PaymentProduct | null>(null)
  const [formBotId, setFormBotId] = useState('')
  const [title, setTitle] = useState('')
  const [price, setPrice] = useState('')
  const [currency, setCurrency] = useState('XTR')
  const [payload, setPayload] = useState('')
  const [description, setDescription] = useState('')
  const [invoiceProductId, setInvoiceProductId] = useState('')
  const [invoiceChatId, setInvoiceChatId] = useState('')

  const productsQuery = useQuery({ queryKey: ['payment-products'], queryFn: fetchPaymentProducts })
  const ordersQuery = useQuery({ queryKey: ['payment-orders'], queryFn: fetchPaymentOrders })
  const botsQuery = useQuery({ queryKey: ['bots'], queryFn: fetchBots })

  const invalidate = () => {
    void queryClient.invalidateQueries({ queryKey: ['payment-products'] })
    void queryClient.invalidateQueries({ queryKey: ['payment-orders'] })
  }

  const saveMutation = useMutation({
    mutationFn: () => {
      const body: PaymentProductPayload = {
        bot_id: formBotId,
        title: title.trim(),
        price: Number(price),
        currency: currency.trim().toUpperCase() || 'XTR',
        payload: payload.trim() || undefined,
        description: description.trim() || undefined,
      }
      return editing ? updatePaymentProduct(editing.id, body) : createPaymentProduct(body)
    },
    onSuccess: () => {
      pushToast(editing ? 'Produk diperbarui.' : 'Produk dibuat.', 'success')
      setShowForm(false)
      setEditing(null)
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string | number) => deletePaymentProduct(id),
    onSuccess: () => {
      pushToast('Produk dihapus.', 'success')
      setDeleting(null)
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const invoiceMutation = useMutation({
    mutationFn: () => sendPaymentInvoice(invoiceProductId, invoiceChatId.trim()),
    onSuccess: () => {
      pushToast('Invoice dikirim ke chat tujuan.', 'success')
      setInvoiceChatId('')
      void queryClient.invalidateQueries({ queryKey: ['payment-orders'] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const openForm = (p?: PaymentProduct) => {
    setEditing(p ?? null)
    setFormBotId(p?.bot_id !== undefined && p.bot_id !== null ? String(p.bot_id) : '')
    setTitle(p ? productTitle(p) : '')
    setPrice(p ? String(productPrice(p)) : '')
    setCurrency(p?.currency ?? 'XTR')
    setPayload(p?.payload ?? '')
    setDescription(p?.description ?? '')
    setShowForm(true)
  }

  const products = productsQuery.data ?? []
  const orders: PaymentOrder[] = ordersQuery.data ?? []
  const bots = botsQuery.data ?? []
  const priceValid = price.trim() !== '' && Number(price) > 0
  const invoiceChatValid = /^-?\d+$/.test(invoiceChatId.trim())

  if (productsQuery.isLoading && ordersQuery.isLoading) return <LoadingSpinner label="Memuat pembayaran…" />
  if (productsQuery.isError && ordersQuery.isError) {
    return (
      <div>
        <PageHeader title="Payments" subtitle="Produk dan pembayaran Telegram Stars" />
        <ErrorState message={(productsQuery.error as Error).message} onRetry={() => void productsQuery.refetch()} />
      </div>
    )
  }

  return (
    <div className="space-y-4">
      <PageHeader
        title="Payments"
        subtitle="Produk dan pembayaran Telegram Stars"
        action={
          <button
            type="button"
            onClick={() => openForm()}
            className="flex items-center gap-1 rounded-full bg-primary px-4 py-2 text-sm font-semibold text-white"
          >
            <Plus className="h-4 w-4" /> Buat Produk
          </button>
        }
      />

      <Card>
        <p className="flex items-start gap-2 text-sm text-ink/70 dark:text-darktext/70">
          <Star className="mt-0.5 h-4 w-4 shrink-0 text-amber-500" />
          <span>
            Pembayaran Telegram Stars (XTR) didahulukan. Untuk Stars, provider_token dikosongkan
            oleh server — pengguna membayar langsung dengan Stars di Telegram.
          </span>
        </p>
      </Card>

      <h2 className="font-bold">Produk</h2>
      {productsQuery.isLoading ? (
        <LoadingSpinner label="Memuat produk…" />
      ) : productsQuery.isError ? (
        <ErrorState message={(productsQuery.error as Error).message} onRetry={() => void productsQuery.refetch()} />
      ) : products.length === 0 ? (
        <EmptyState
          icon={<CreditCard className="h-10 w-10 text-primary" />}
          title="Belum ada produk"
          description="Buat produk pertama Anda — misalnya paket premium seharga 100 Stars — lalu kirim invoicenya ke chat pengguna."
          action={
            <button
              type="button"
              onClick={() => openForm()}
              className="mt-2 rounded-full bg-primary px-4 py-2 text-sm font-semibold text-white"
            >
              Buat Produk
            </button>
          }
        />
      ) : (
        <div className="space-y-2">
          {products.map((p) => (
            <Card key={String(p.id)}>
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <p className="truncate font-bold">{productTitle(p)}</p>
                  <p className="text-sm text-ink/60 dark:text-darktext/60">{formatPrice(productPrice(p), p.currency)}</p>
                  {p.payload && (
                    <p className="truncate font-mono text-xs text-ink/50 dark:text-darktext/50">payload: {p.payload}</p>
                  )}
                  {p.description && <p className="mt-1 text-sm text-ink/70 dark:text-darktext/70">{p.description}</p>}
                </div>
                <div className="flex shrink-0 gap-1">
                  <button
                    type="button"
                    onClick={() => openForm(p)}
                    className="rounded-full bg-ink/10 px-3 py-1.5 text-xs font-semibold dark:bg-darktext/10"
                  >
                    Edit
                  </button>
                  <button
                    type="button"
                    aria-label="Hapus produk"
                    onClick={() => setDeleting(p)}
                    className="rounded-full p-1.5 text-danger hover:bg-danger/10"
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
              </div>
            </Card>
          ))}
        </div>
      )}

      <Card>
        <h2 className="mb-3 font-bold">Kirim Invoice ke Chat</h2>
        <div className="space-y-3">
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Produk</span>
            <select value={invoiceProductId} onChange={(e) => setInvoiceProductId(e.target.value)} className={inputCls}>
              <option value="">Pilih produk…</option>
              {products.map((p) => (
                <option key={String(p.id)} value={String(p.id)}>
                  {productTitle(p)} — {formatPrice(productPrice(p), p.currency)}
                </option>
              ))}
            </select>
          </label>
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Chat ID penerima (numerik)</span>
            <input
              value={invoiceChatId}
              onChange={(e) => setInvoiceChatId(e.target.value)}
              placeholder="mis. 123456789"
              inputMode="numeric"
              className={inputCls}
            />
            {invoiceChatId.trim() && !invoiceChatValid && (
              <span className="mt-1 block text-xs text-danger">Chat ID harus angka.</span>
            )}
          </label>
          <button
            type="button"
            onClick={() => invoiceMutation.mutate()}
            disabled={!invoiceProductId || !invoiceChatValid || invoiceMutation.isPending}
            className="flex items-center gap-1 rounded-full bg-primary px-4 py-2 text-sm font-bold text-white disabled:opacity-50"
          >
            <Send className="h-4 w-4" />
            {invoiceMutation.isPending ? 'Mengirim…' : 'Kirim Invoice'}
          </button>
        </div>
      </Card>

      <h2 className="font-bold">Riwayat Order</h2>
      {ordersQuery.isLoading ? (
        <LoadingSpinner label="Memuat order…" />
      ) : ordersQuery.isError ? (
        <ErrorState message={(ordersQuery.error as Error).message} onRetry={() => void ordersQuery.refetch()} />
      ) : orders.length === 0 ? (
        <EmptyState title="Belum ada order" description="Order muncul setelah ada pengguna yang membayar invoice." />
      ) : (
        <div className="space-y-2">
          {orders.map((o) => (
            <Card key={String(o.id)}>
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0 text-sm">
                  <p className="truncate font-semibold">
                    {products.find((p) => String(p.id) === String(o.product_id))?.title ??
                      `Order ${o.id}`}
                  </p>
                  <p className="text-ink/60 dark:text-darktext/60">
                    {formatPrice(o.amount ?? o.price ?? 0, o.currency)} · chat {o.chat_id ?? '—'}
                  </p>
                  <p className="text-xs text-ink/50 dark:text-darktext/50">
                    {o.created_at ? new Date(o.created_at).toLocaleString('id-ID') : ''}
                  </p>
                </div>
                {orderStatusBadge(o.status)}
              </div>
            </Card>
          ))}
        </div>
      )}

      <Modal open={showForm} onClose={() => setShowForm(false)} title={editing ? 'Edit Produk' : 'Buat Produk'}>
        <div className="space-y-3">
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Bot penjual</span>
            <select
              value={formBotId}
              onChange={(e) => setFormBotId(e.target.value)}
              disabled={editing !== null}
              className={inputCls}
            >
              <option value="">Pilih bot…</option>
              {bots.map((b) => (
                <option key={String(b.id)} value={String(b.id)}>
                  {b.name ?? b.username ?? `Bot ${b.id}`}
                  {b.username ? ` (@${b.username})` : ''}
                </option>
              ))}
            </select>
            {editing !== null && (
              <span className="mt-1 block text-xs text-ink/50 dark:text-darktext/50">
                Bot penjual tidak bisa diubah setelah produk dibuat.
              </span>
            )}
          </label>
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Judul produk</span>
            <input value={title} onChange={(e) => setTitle(e.target.value)} placeholder="mis. Paket Premium" className={inputCls} />
          </label>
          <div className="grid grid-cols-2 gap-3">
            <label className="block text-sm">
              <span className="mb-1 block font-medium">Harga</span>
              <input
                value={price}
                onChange={(e) => setPrice(e.target.value)}
                placeholder="100"
                inputMode="decimal"
                className={inputCls}
              />
            </label>
            <label className="block text-sm">
              <span className="mb-1 block font-medium">Mata uang</span>
              <select value={currency} onChange={(e) => setCurrency(e.target.value)} className={inputCls}>
                <option value="XTR">XTR (Stars)</option>
                <option value="IDR">IDR</option>
                <option value="USD">USD</option>
              </select>
            </label>
          </div>
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Payload (opsional)</span>
            <input
              value={payload}
              onChange={(e) => setPayload(e.target.value)}
              placeholder="mis. premium-1-bulan"
              className={inputCls}
            />
          </label>
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Deskripsi (opsional)</span>
            <textarea value={description} onChange={(e) => setDescription(e.target.value)} rows={2} className={inputCls} />
          </label>
          <button
            type="button"
            disabled={!title.trim() || !priceValid || (!editing && !formBotId) || saveMutation.isPending}
            onClick={() => saveMutation.mutate()}
            className="w-full rounded-full bg-primary py-2.5 text-sm font-bold text-white disabled:opacity-50"
          >
            {saveMutation.isPending ? 'Menyimpan…' : editing ? 'Simpan Perubahan' : 'Buat Produk'}
          </button>
        </div>
      </Modal>

      <ConfirmDialog
        open={deleting !== null}
        onClose={() => setDeleting(null)}
        onConfirm={() => deleting && deleteMutation.mutate(deleting.id)}
        title="Hapus Produk"
        message={`Hapus produk "${deleting ? productTitle(deleting) : ''}"? Invoice yang sudah terkirim tidak ikut dihapus.`}
        busy={deleteMutation.isPending}
      />
    </div>
  )
}
