import { request } from './apiClient'

export async function uploadDeck(file) {
  const formData = new FormData()
  formData.append('file', file)

  return request('/api/decks/upload', {
    method: 'POST',
    body: formData,
  })
}
