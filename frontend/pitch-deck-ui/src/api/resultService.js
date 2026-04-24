import { request } from './apiClient'

export async function healthCheck() {
  return request('/api/health')
}

export async function searchDecks(query, limit = 5) {
  return request('/api/decks/search', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ query, limit }),
  })
}

export async function getJobStatus(jobId) {
  return request(`/api/jobs/status/${jobId}`)
}

