/**
 * Pertahanan berlapis di UI: sembunyikan pola token bot Telegram
 * (`<id numerik>:<secret>`) dari teks yang ditampilkan, mis. pesan
 * notifikasi yang memuat URL API. Backend juga menyanitasi, tetapi UI
 * tidak boleh menampilkan token utuh bila suatu saat lolos.
 */
const BOT_TOKEN_PATTERN = /\d{5,}:[A-Za-z0-9_-]{20,}/g

export const REDACTED_TOKEN_PLACEHOLDER = '[token disembunyikan]'

export function redactBotTokens(text: string): string {
  return text.replace(BOT_TOKEN_PATTERN, REDACTED_TOKEN_PLACEHOLDER)
}
