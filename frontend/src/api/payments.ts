import { apiRequest, asArray } from './client'
import type { PaymentOrder, PaymentProduct } from '../types'

export async function fetchPaymentProducts(): Promise<PaymentProduct[]> {
  const data = await apiRequest<unknown>('/api/payments/products')
  return asArray<PaymentProduct>(data, ['products', 'items', 'data'])
}

export interface PaymentProductPayload {
  bot_id: string | number
  title: string
  price: number
  currency: string
  payload?: string
  description?: string
}

export async function createPaymentProduct(payload: PaymentProductPayload): Promise<PaymentProduct> {
  return apiRequest<PaymentProduct>('/api/payments/products', { method: 'POST', body: payload })
}

export async function updatePaymentProduct(
  id: string | number,
  payload: Partial<PaymentProductPayload>,
): Promise<PaymentProduct> {
  return apiRequest<PaymentProduct>(`/api/payments/products/${id}`, { method: 'PUT', body: payload })
}

export async function deletePaymentProduct(id: string | number): Promise<void> {
  await apiRequest(`/api/payments/products/${id}`, { method: 'DELETE' })
}

export async function fetchPaymentOrders(): Promise<PaymentOrder[]> {
  const data = await apiRequest<unknown>('/api/payments/orders')
  return asArray<PaymentOrder>(data, ['orders', 'items', 'data'])
}

export async function sendPaymentInvoice(
  productId: string | number,
  chatId: string | number,
): Promise<unknown> {
  return apiRequest(`/api/payments/${productId}/invoice`, {
    method: 'POST',
    body: { chat_id: chatId },
  })
}

export function productTitle(p: PaymentProduct): string {
  return p.title ?? p.name ?? `Produk ${p.id}`
}

export function productPrice(p: PaymentProduct): number {
  return p.price ?? p.amount ?? 0
}

export function formatPrice(amount: number, currency?: string): string {
  const cur = (currency ?? 'XTR').toUpperCase()
  if (cur === 'XTR') return `${amount} ⭐ Stars`
  return `${amount} ${cur}`
}
