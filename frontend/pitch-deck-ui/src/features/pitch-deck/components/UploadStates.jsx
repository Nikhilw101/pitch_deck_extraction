import { Alert, Button, CircularProgress, LinearProgress } from '@mui/material'
import { BarChart3, CheckCircle2, ClipboardCopy, FileUp, FileX, Table2, UploadCloud } from 'lucide-react'
import { toast } from 'sonner'
import { CardContent, CardHeader } from '../../../components/ui/card'
import { Badge } from '../../../components/ui/badge'
import { Progress } from '../../../components/ui/progress'
import { formatBytes, getFileExt } from '../../../lib/utils'

export function IdleState({ onBrowse }) {
  return (
    <CardContent className="p-6">
      <div className="w-full rounded-2xl border-2 border-dashed border-white/20 bg-slate-900/40 p-12 text-center transition hover:border-indigo-400/80 hover:bg-indigo-500/5">
        <UploadCloud className="mx-auto mb-3 h-10 w-10 text-indigo-300" />
        <p className="text-lg font-medium">Drag & drop your file here</p>
        <p className="mt-1 text-sm text-slate-400">Supports PDF and PPTX</p>
        <Button type="button" variant="contained" sx={{ mt: 3 }} startIcon={<FileUp size={16} />} onClick={onBrowse}>
          Browse File
        </Button>
      </div>
    </CardContent>
  )
}

export function SelectedState({ file, onReset, onAnalyze, disabled }) {
  return (
    <CardContent className="space-y-4 p-6">
      <div className="rounded-xl border border-white/15 bg-slate-800/70 p-4">
        <div className="flex items-start justify-between gap-4">
          <div className="flex gap-3">
            <div className="rounded-lg border border-indigo-300/20 bg-indigo-500/10 px-3 py-2 text-xs font-semibold uppercase text-indigo-200">
              {getFileExt(file?.name)}
            </div>
            <div>
              <p className="font-medium text-slate-100">{file?.name}</p>
              <p className="text-sm text-slate-400">{formatBytes(file?.size || 0)}</p>
            </div>
          </div>
          <Button color="error" onClick={onReset} startIcon={<FileX size={16} />} disabled={disabled}>
            Remove
          </Button>
        </div>
      </div>
      <div className="flex flex-wrap gap-3">
        <Button variant="outlined" color="inherit" onClick={onReset} disabled={disabled}>
          Cancel
        </Button>
        <Button variant="contained" onClick={onAnalyze} disabled={disabled}>
          Analyze Deck
        </Button>
      </div>
    </CardContent>
  )
}

export function ProcessingState({ dots, progress, message, log = [] }) {
  return (
    <CardContent className="flex min-h-[500px] flex-col items-center justify-center space-y-8 p-12 text-center">
      <div className="relative">
        <div className="absolute -inset-4 rounded-full bg-indigo-500/20 blur-xl animate-pulse" />
        <CircularProgress size={80} thickness={4} sx={{ color: '#818cf8' }} />
      </div>
      
      <div className="space-y-4">
        <h3 className="text-2xl font-bold tracking-tight text-white">
          Analyzing your pitch deck<span className="text-indigo-400">{dots}</span>
        </h3>
        <div className="flex flex-col items-center gap-2">
          <Badge variant="outline" className="border-indigo-500/30 bg-indigo-500/10 text-indigo-300 px-4 py-1 text-sm font-medium">
            {message || 'Processing...'}
          </Badge>
          <p className="max-w-md text-slate-400">
            Our AI is currently extracting structure, identifying investment signals, and generating summaries.
          </p>
        </div>
      </div>

      <div className="w-full max-w-md space-y-6">
        <div className="space-y-2">
          <div className="flex justify-between text-xs font-medium text-slate-500">
            <span>Overall Progress</span>
            <span>{progress}%</span>
          </div>
          <LinearProgress 
            variant="determinate" 
            value={progress} 
            sx={{ 
              height: 8, 
              borderRadius: 4,
              backgroundColor: 'rgba(255,255,255,0.05)',
              '& .MuiLinearProgress-bar': {
                borderRadius: 4,
                background: 'linear-gradient(90deg, #6366f1 0%, #a855f7 100%)'
              }
            }} 
          />
        </div>

        {/* Activity Log */}
        <div className="rounded-xl border border-white/10 bg-black/20 p-4 text-left font-mono text-[11px]">
          <div className="mb-2 border-b border-white/5 pb-1 text-[10px] uppercase tracking-wider text-slate-500">
            Activity Log
          </div>
          <div className="h-24 overflow-y-auto space-y-1 custom-scrollbar">
            {log.map((item, i) => (
              <div key={i} className="flex gap-2 text-slate-300">
                <span className="text-indigo-400/60">[{new Date().toLocaleTimeString([], { hour12: false })}]</span>
                <span className={i === log.length - 1 ? "text-indigo-300 font-bold" : "text-slate-400 opacity-60"}>
                  {i === log.length - 1 ? "> " : "- "}{item}
                </span>
              </div>
            ))}
          </div>
        </div>
      </div>

      <div className="rounded-lg border border-blue-500/20 bg-blue-500/5 p-4 text-sm text-blue-200">
        Processing is running in the background. You don't need to stay on this tab.
      </div>
    </CardContent>
  )
}



export function SuccessState({ deckId, fileName, summary, onReset }) {
  return (
    <>
      <CardHeader>
        <Alert icon={<CheckCircle2 size={18} />} severity="success">
          Your deck has been successfully processed
        </Alert>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="rounded-xl border border-white/15 bg-slate-800/50 p-4">
          <div className="flex flex-wrap items-center justify-between gap-3 text-sm">
            <p>
              Deck ID: <span className="font-mono text-indigo-300">{deckId}</span>
            </p>
            <Button
              size="small"
              startIcon={<ClipboardCopy size={14} />}
              onClick={() => {
                navigator.clipboard.writeText(deckId).catch(() => {})
                toast.success('Deck ID copied')
              }}
            >
              Copy
            </Button>
          </div>
          <p className="mt-2 text-sm text-slate-300">File: {fileName || '-'}</p>
          <p className="text-sm text-slate-300">Total slides: {summary?.total_slides ?? '-'}</p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Badge><Table2 className="mr-1 h-3.5 w-3.5" />{summary?.metadata?.has_tables ? 'Tables found' : 'No tables'}</Badge>
          <Badge><BarChart3 className="mr-1 h-3.5 w-3.5" />{summary?.metadata?.has_charts ? 'Charts found' : 'No charts'}</Badge>
          <Badge>{summary?.metadata?.has_speaker_notes ? 'Notes available' : 'No speaker notes'}</Badge>
        </div>
        <Button variant="outlined" color="inherit" onClick={onReset}>
          Upload another deck
        </Button>
      </CardContent>
    </>
  )
}

export function ErrorState({ errorMsg, requestId, onReset }) {
  return (
    <CardContent className="p-6">
      <Alert severity="error" sx={{ mb: 2 }}>
        {errorMsg}
      </Alert>
      <p className="mb-3 text-xs text-slate-400">Request ID: {requestId}</p>
      <Button variant="contained" color="error" onClick={onReset}>
        Try Again
      </Button>
    </CardContent>
  )
}
