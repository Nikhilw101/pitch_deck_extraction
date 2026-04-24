import { useEffect, useMemo, useRef, useState } from 'react'
import { AnimatePresence, motion as Motion } from 'framer-motion'
import { toast } from 'sonner'
import { Card } from '../../components/ui/card'
import { getFileExt } from '../../lib/utils'
import { API_BASE_URL } from '../../api/apiClient'
import { uploadDeck } from '../../api/uploadService'
import { healthCheck, getJobStatus } from '../../api/resultService'
import { ALLOWED_EXTENSIONS, MAX_FILE_SIZE_BYTES, UPLOAD_STATUS } from './constants'
import { LandingHeader } from './components/LandingHeader'
import { ErrorState, IdleState, ProcessingState, SelectedState, SuccessState } from './components/UploadStates'
import { ResultsSection } from './components/ResultsSection'

export default function PitchDeckAnalyzerPage() {
  const [status, setStatus] = useState(UPLOAD_STATUS.IDLE)
  const [file, setFile] = useState(null)
  const [errorMsg, setErrorMsg] = useState('The file could not be processed. Please retry.')
  const [requestId, setRequestId] = useState('-')
  const [uploadResponse, setUploadResponse] = useState(null)
  const [backendReady, setBackendReady] = useState(true)
  const [healthMsg, setHealthMsg] = useState('')
  const [dots, setDots] = useState('.')
  const [progress, setProgress] = useState(0)
  const [progressMessage, setProgressMessage] = useState('Initializing...')
  const [statusLog, setStatusLog] = useState([])
  const resultsRef = useRef(null)
  const inputRef = useRef(null)

  useEffect(() => {
    let isMounted = true
    async function checkBackend() {
      try {
        const response = await healthCheck()
        if (!isMounted) return
        setBackendReady(true)
        setHealthMsg(response?.message || 'Backend healthy')
      } catch (error) {
        if (!isMounted) return
        setBackendReady(false)
        setHealthMsg(error.message || 'Backend unavailable')
      }
    }
    checkBackend()
    return () => {
      isMounted = false
    }
  }, [])

  useEffect(() => {
    if (status !== UPLOAD_STATUS.PROCESSING) return undefined
    const dotTimer = setInterval(() => {
      setDots((prev) => (prev.length >= 3 ? '.' : `${prev}.`))
    }, 450)
    const progressTimer = setInterval(() => {
      setProgress((prev) => Math.min(prev + 2, 95))
    }, 1000)
    return () => {
      clearInterval(dotTimer)
      clearInterval(progressTimer)
    }
  }, [status])

  const deckId = useMemo(() => uploadResponse?.data?.deck_id || '-', [uploadResponse])

  function validateAndSetFile(nextFile) {
    if (!nextFile) return
    if (!backendReady) {
      toast.error('Backend is offline. Start backend and retry.')
      return
    }
    const ext = getFileExt(nextFile.name)
    if (!ALLOWED_EXTENSIONS.includes(ext)) {
      toast.error('Invalid file type. Please upload PDF or PPTX.')
      return
    }
    if (nextFile.size > MAX_FILE_SIZE_BYTES) {
      toast.error('File is too large. Max allowed size is 50 MB.')
      return
    }
    setFile(nextFile)
    setStatus(UPLOAD_STATUS.SELECTED)
  }

  async function onStartAnalyze() {
    if (!file) {
      toast.error('Please select a file before analyzing.')
      return
    }
    if (!backendReady) {
      toast.error('Backend is offline. Start backend and retry.')
      return
    }

    setStatus(UPLOAD_STATUS.PROCESSING)
    setErrorMsg('')
    setRequestId('-')
    setUploadResponse(null)
    setDots('.')
    setProgress(0)
    setStatusLog(['Analyzing request...'])

    try {
      const uploadPayload = await uploadDeck(file)
      const jobId = uploadPayload.data?.job_id
      setRequestId(uploadPayload.request_id || '-')

      if (!jobId) {
        throw new Error('Failed to start analysis: No job ID returned.')
      }

      // Polling loop — no timeout, poll until job finishes or errors
      let isCompleted = false
      let pollCount = 0
      let consecutiveErrors = 0
      
      while (!isCompleted) { // No timeout — wait as long as needed
        pollCount += 1
        await new Promise(resolve => setTimeout(resolve, 5000))
        
        try {
          const statusResponse = await getJobStatus(jobId)
          const statusData = statusResponse.data
          consecutiveErrors = 0 // Reset error count on success

          if (statusData.Completed) {
            const finalResult = {
              ...statusResponse,
              data: statusData.Completed
            }
            setUploadResponse(finalResult)
            setProgress(100)
            setStatus(UPLOAD_STATUS.SUCCESS)
            toast.success('Deck analyzed successfully')
            isCompleted = true
            setTimeout(() => resultsRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' }), 150)
          } else if (statusData.Failed) {
            throw new Error(statusData.Failed.error || 'Job failed on server')
          } else if (statusData.Processing) {
            const newMsg = statusData.Processing.progress || 'Processing...'
            setProgressMessage(newMsg)
            setStatusLog(prev => {
              if (prev[prev.length - 1] !== newMsg) {
                return [...prev, newMsg]
              }
              return prev
            })
            // Slowly increment progress as we poll
            setProgress(prev => Math.min(prev + 5, 95))
          }
        } catch (pollError) {
          consecutiveErrors++
          console.warn(`Polling error (attempt ${pollCount}, error ${consecutiveErrors}):`, pollError)
          
          // Only fail if we get 5 errors in a row (transient network issues are fine)
          if (consecutiveErrors >= 5) {
            throw pollError
          }
          
          // Otherwise, just update the message and keep trying
          setProgressMessage('Waiting for server connection...')
        }
      }
      
    } catch (error) {
      setRequestId(error.requestId || '-')
      setErrorMsg(error.message || 'Processing failed. Please retry.')
      setStatus(UPLOAD_STATUS.ERROR)
      toast.error(error.message || 'Failed to analyze deck')
    }
  }

  function resetToIdle() {
    setFile(null)
    setStatus(UPLOAD_STATUS.IDLE)
    setUploadResponse(null)
    setErrorMsg('The file could not be processed. Please retry.')
    setRequestId('-')
    setDots('.')
    setProgress(0)
    if (inputRef.current) inputRef.current.value = ''
    window.scrollTo({ top: 0, behavior: 'smooth' })
  }

  return (
    <main className="min-h-screen bg-[radial-gradient(circle_at_top,#1a2550_0%,#070b14_45%,#05070d_100%)] px-4 py-12 text-slate-100">
      <div className="mx-auto max-w-4xl space-y-8">
        <LandingHeader />
        <p className={`text-center text-sm ${backendReady ? 'text-emerald-300' : 'text-rose-300'}`}>
          Backend: {backendReady ? `Connected (${API_BASE_URL})` : `Disconnected (${API_BASE_URL})`} - {healthMsg}
        </p>

        <Card className="overflow-hidden">
          <AnimatePresence mode="wait">
            {status === UPLOAD_STATUS.IDLE && (
              <Motion.div key="idle" initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }}>
                <IdleState onBrowse={() => inputRef.current?.click()} />
              </Motion.div>
            )}

            {status === UPLOAD_STATUS.SELECTED && file && (
              <Motion.div key="selected" initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }}>
                <SelectedState file={file} onReset={resetToIdle} onAnalyze={onStartAnalyze} disabled={!backendReady} />
              </Motion.div>
            )}

            {status === UPLOAD_STATUS.PROCESSING && (
              <Motion.div key="processing" initial={{ opacity: 0 }} animate={{ opacity: 1 }}>
                <ProcessingState dots={dots} progress={progress} message={progressMessage} log={statusLog} />
              </Motion.div>
            )}

            {status === UPLOAD_STATUS.SUCCESS && (
              <Motion.div key="success" initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }}>
                <SuccessState deckId={deckId} fileName={file?.name} summary={uploadResponse?.data} onReset={resetToIdle} />
              </Motion.div>
            )}

            {status === UPLOAD_STATUS.ERROR && (
              <Motion.div key="error" initial={{ opacity: 0 }} animate={{ opacity: 1 }}>
                <ErrorState errorMsg={errorMsg} requestId={requestId} onReset={resetToIdle} />
              </Motion.div>
            )}
          </AnimatePresence>
        </Card>

        <input
          ref={inputRef}
          type="file"
          className="hidden"
          accept=".pdf,.ppt,.pptx"
          onChange={(event) => validateAndSetFile(event.target.files?.[0])}
        />

        {status === UPLOAD_STATUS.SUCCESS && (
          <ResultsSection
            resultsRef={resultsRef}
            insights={[
              { label: 'Request ID', value: requestId },
              { label: 'Status', value: uploadResponse?.status || '-' },
              { label: 'Message', value: uploadResponse?.message || '-' },
              { label: 'Indexing Status', value: uploadResponse?.data?.indexing?.status || '-' },
            ]}
            jsonData={uploadResponse}
          />
        )}
      </div>
    </main>
  )
}
