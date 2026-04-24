import { Sparkles } from 'lucide-react'

export function LandingHeader() {
  return (
    <header className="space-y-3 text-center">
      <div className="inline-flex items-center gap-2 rounded-full border border-indigo-400/40 bg-indigo-500/10 px-3 py-1 text-xs text-indigo-200">
        <Sparkles className="h-3.5 w-3.5" />
        AI-Powered Analysis
      </div>
      <h1 className="text-4xl font-light tracking-tight sm:text-5xl">Pitch Deck Analyzer</h1>
      <p className="mx-auto max-w-2xl text-sm text-slate-300 sm:text-base">
        Upload your PDF or PPT and extract structured insights instantly.
      </p>
    </header>
  )
}
