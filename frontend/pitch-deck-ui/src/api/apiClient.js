const DEFAULT_BASE_URL = 'http://127.0.0.1:3000'

export const API_BASE_URL = (import.meta.env.VITE_API_BASE_URL || DEFAULT_BASE_URL).trim()

function isJsonResponse(response) {
  const contentType = response.headers.get('content-type') || ''
  return contentType.toLowerCase().includes('application/json')
}

export async function request(path, options = {}) {
  const url = `${API_BASE_URL}${path}`
  console.info(`[API] Calling API: ${options.method || 'GET'} ${url}`)

  let response
  try {
    response = await fetch(url, options)
  } catch (error) {
    console.error(`[API] API Failed (network): ${options.method || 'GET'} ${url}`, error)
    throw new Error('Network error: unable to reach backend service.')
  }

  if (!isJsonResponse(response)) {
    console.error(`[API] API Failed (non-json): ${response.status} ${url}`)
    throw new Error(`Unexpected response from backend (HTTP ${response.status}).`)
  }

  const payload = await response.json()

  if (!response.ok || payload?.status !== 'success') {
    const message =
      payload?.error?.message ||
      payload?.message ||
      `Backend request failed (HTTP ${response.status}).`
    console.error(`[API] API Failed: ${response.status} ${url}`, payload)
    const enrichedError = new Error(message)
    enrichedError.requestId = payload?.request_id || null
    enrichedError.statusCode = response.status
    throw enrichedError
  }

  console.info(`[API] API Success: ${response.status} ${url}`, payload)
  return payload
}
