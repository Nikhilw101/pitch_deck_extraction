import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from '../../../components/ui/accordion'
import { Card, CardContent, CardTitle } from '../../../components/ui/card'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../../../components/ui/tabs'
import { Badge } from '../../../components/ui/badge'
import { Download } from 'lucide-react'
import { toast } from 'sonner'
import { exportReportPdf } from '../lib/exportReportPdf'

function pct(value) {
  if (typeof value !== 'number' || Number.isNaN(value)) return '-'
  return `${(value * 100).toFixed(1)}%`
}

function safeNumber(value) {
  if (typeof value !== 'number' || Number.isNaN(value)) return null
  return value
}

function DeckReport({ report }) {
  if (!report) {
    return (
      <Card className="border-white/10 bg-slate-900/60">
        <CardContent className="p-4 text-sm text-slate-300">No structured deck report returned by backend.</CardContent>
      </Card>
    )
  }

  const sections = Array.isArray(report.sections) ? report.sections : []

  return (
    <div className="space-y-4">
      <Card className="border-white/10 bg-slate-900/60">
        <CardTitle className="px-5 pt-5">Deck Report</CardTitle>
        <CardContent className="space-y-3 p-5 text-sm text-slate-200">
          <div className="flex flex-wrap items-center gap-2">
            <Badge>Company: {report.company_name || '—'}</Badge>
            <Badge variant="secondary">Deck: {report.filename || '—'}</Badge>
            <Badge variant="secondary">Overall confidence: {pct(report.confidence_score)}</Badge>
          </div>
          {report.company_website && (
            <p className="text-slate-300">
              Website: <span className="font-mono text-emerald-300">{report.company_website}</span>
            </p>
          )}
          {report.extraction_timestamp && <p className="text-slate-400">Extracted at: {report.extraction_timestamp}</p>}
        </CardContent>
      </Card>

      {report.overall_summary && (
        <Card className="border-white/10 bg-slate-900/60">
          <CardTitle className="px-5 pt-5">Overall Summary</CardTitle>
          <CardContent className="p-5 text-sm text-slate-300 whitespace-pre-wrap">{report.overall_summary}</CardContent>
        </Card>
      )}

      <Card className="border-white/10 bg-slate-900/60">
        <CardTitle className="px-5 pt-5">Score Breakdown</CardTitle>
        <CardContent className="grid gap-2 p-5 text-sm text-slate-300 md:grid-cols-2">
          <div>Validation: {pct(report.score_breakdown?.validation_score)}</div>
          <div>LLM confidence: {pct(report.score_breakdown?.llm_confidence)}</div>
          <div>Completeness: {pct(report.score_breakdown?.completeness_score)}</div>
          <div>Web consistency: {pct(report.score_breakdown?.web_consistency)}</div>
          <div>Threshold score: {pct(report.score_breakdown?.threshold_score)}</div>
          <div>
            Final score:{' '}
            <span className="font-medium text-slate-100">{pct(report.score_breakdown?.final_score ?? report.confidence_score)}</span>
          </div>
        </CardContent>
      </Card>

      <Card className="border-white/10 bg-slate-900/60">
        <CardTitle className="px-5 pt-5">Sections</CardTitle>
        <CardContent className="p-4">
          <Accordion type="multiple" className="space-y-2">
            {sections.map((section, idx) => {
              const validationScore = safeNumber(section?.validation?.score)
              const sectionConfidence = safeNumber(section?.confidence)
              const signals = Array.isArray(section?.signals) ? section.signals : []
              const flags = Array.isArray(section?.red_flags) ? section.red_flags : []
              const thresholds = Array.isArray(section?.threshold_flags) ? section.threshold_flags : []
              return (
                <AccordionItem key={`${section?.section_name || 'section'}-${idx}`} value={`${idx}`} className="rounded-xl border border-white/10 bg-slate-950/40 px-3">
                  <AccordionTrigger>
                    <div className="flex w-full flex-wrap items-center justify-between gap-2 pr-3 text-left">
                      <span className="font-medium text-slate-100">{section?.section_name || `Section ${idx + 1}`}</span>
                      <span className="flex flex-wrap items-center gap-2">
                        <Badge variant="secondary">Confidence: {pct(sectionConfidence ?? 0)}</Badge>
                        <Badge variant="secondary">Validation: {pct(validationScore ?? 0)}</Badge>
                        <Badge variant="secondary">Signals: {signals.length}</Badge>
                        <Badge variant="secondary">Red flags: {flags.length}</Badge>
                      </span>
                    </div>
                  </AccordionTrigger>
                  <AccordionContent>
                    <div className="space-y-3 pb-2 text-sm text-slate-300">
                      {section?.web_validation && (
                        <div>
                          <span className="font-medium text-slate-200">Web validation:</span> {section.web_validation}
                        </div>
                      )}

                      {thresholds.length > 0 && (
                        <div>
                          <div className="font-medium text-slate-200">Threshold flags</div>
                          <ul className="list-disc space-y-1 pl-5">
                            {thresholds.map((t) => (
                              <li key={t}>{t}</li>
                            ))}
                          </ul>
                        </div>
                      )}

                      {section?.summary && (
                        <div>
                          <div className="font-medium text-slate-200">Section summary</div>
                          <div className="whitespace-pre-wrap">{section.summary}</div>
                        </div>
                      )}

                      <div>
                        <div className="font-medium text-slate-200">Extracted fields</div>
                        {(() => {
                          const data = section?.data;
                          // Handle case where data is an array (fallback for bad LLM output)
                          const dataObj = Array.isArray(data) 
                            ? data.reduce((acc, item) => {
                                if (item.FieldName) acc[item.FieldName] = item;
                                return acc;
                              }, {})
                            : data;
                            
                          if (!dataObj || Object.keys(dataObj).length === 0) {
                            return <div className="mt-2 text-sm italic text-slate-500">No structured data extracted</div>;
                          }

                          return (
                            <div className="mt-2 grid gap-3 md:grid-cols-2">
                              {Object.entries(dataObj).map(([key, field]) => {
                                const isObj = typeof field === 'object' && field !== null;
                                const val = isObj ? field.value : field;
                                const source = isObj ? field.source_text : null;
                                const conf = isObj ? field.confidence : undefined;
                                const slide = isObj ? field.slide_number : undefined;
                                
                                return (
                                  <Card key={key} className="border-white/10 bg-slate-900/40">
                                    <CardContent className="p-3">
                                      <div className="text-xs font-semibold text-slate-400">{key}</div>
                                      <div className="mt-1 text-sm text-slate-200">
                                        {val !== null && val !== undefined && val !== "" ? String(val) : <span className="italic text-slate-500">Not found</span>}
                                      </div>
                                      {source && source !== "" && (
                                        <div className="mt-2 text-xs italic text-slate-400">"{source}"</div>
                                      )}
                                      {conf !== undefined && conf !== null && (
                                        <div className="mt-2 flex items-center justify-between border-t border-white/5 pt-2">
                                          <div className="text-[10px] text-slate-500">Confidence: {pct(conf)}</div>
                                          {slide !== undefined && slide !== null && <div className="text-[10px] text-slate-500">Slide {slide}</div>}
                                        </div>
                                      )}
                                    </CardContent>
                                  </Card>
                                );
                              })}
                            </div>
                          );
                        })()}
                      </div>

                      {signals.length > 0 && (
                        <div>
                          <div className="font-medium text-slate-200">Investment signals</div>
                          <div className="space-y-2">
                            {signals.map((s, sIdx) => (
                              <Card key={`${s?.signal_type || 'signal'}-${sIdx}`} className="border-white/10 bg-slate-900/40">
                                <CardContent className="space-y-1 p-3">
                                  <div className="flex flex-wrap items-center justify-between gap-2">
                                    <div className="font-medium text-slate-100">{s?.signal_type || 'Signal'}</div>
                                    <Badge>Confidence: {pct(safeNumber(s?.confidence) ?? 0)}</Badge>
                                  </div>
                                  <div className="text-slate-300">{s?.description || ''}</div>
                                </CardContent>
                              </Card>
                            ))}
                          </div>
                        </div>
                      )}

                      {flags.length > 0 && (
                        <div>
                          <div className="font-medium text-slate-200">Red flags</div>
                          <div className="space-y-2">
                            {flags.map((f, fIdx) => (
                              <Card key={`${f?.flag_type || 'flag'}-${fIdx}`} className="border-white/10 bg-rose-950/20">
                                <CardContent className="space-y-1 p-3">
                                  <div className="flex flex-wrap items-center justify-between gap-2">
                                    <div className="font-medium text-slate-100">{f?.flag_type || 'Flag'}</div>
                                    <Badge variant="secondary">Severity: {f?.severity || '—'}</Badge>
                                  </div>
                                  <div className="text-slate-300">{f?.description || ''}</div>
                                </CardContent>
                              </Card>
                            ))}
                          </div>
                        </div>
                      )}

                      {section?.validation?.errors?.length > 0 && (
                        <div>
                          <div className="font-medium text-slate-200">Validation errors</div>
                          <pre className="max-h-60 overflow-auto rounded-xl border border-white/10 bg-slate-950/70 p-3 text-xs text-amber-200">
                            {JSON.stringify(section.validation.errors, null, 2)}
                          </pre>
                        </div>
                      )}
                    </div>
                  </AccordionContent>
                </AccordionItem>
              )
            })}
          </Accordion>
        </CardContent>
      </Card>

      {(Array.isArray(report.overall_signals) && report.overall_signals.length > 0) || (Array.isArray(report.overall_red_flags) && report.overall_red_flags.length > 0) ? (
        <Card className="border-white/10 bg-slate-900/60">
          <CardTitle className="px-5 pt-5">Overall Signals & Red Flags</CardTitle>
          <CardContent className="space-y-3 p-5 text-sm text-slate-300">
            {Array.isArray(report.overall_signals) && report.overall_signals.length > 0 && (
              <div>
                <div className="font-medium text-slate-200">Signals</div>
                <pre className="max-h-60 overflow-auto rounded-xl border border-white/10 bg-slate-950/70 p-3 text-xs text-emerald-300">
                  {JSON.stringify(report.overall_signals, null, 2)}
                </pre>
              </div>
            )}
            {Array.isArray(report.overall_red_flags) && report.overall_red_flags.length > 0 && (
              <div>
                <div className="font-medium text-slate-200">Red flags</div>
                <pre className="max-h-60 overflow-auto rounded-xl border border-white/10 bg-slate-950/70 p-3 text-xs text-rose-200">
                  {JSON.stringify(report.overall_red_flags, null, 2)}
                </pre>
              </div>
            )}
          </CardContent>
        </Card>
      ) : null}
    </div>
  )
}

export function ResultsSection({ resultsRef, insights, jsonData }) {
  const safeInsights = Array.isArray(insights) ? insights : []
  const report = jsonData?.data?.structured_output || null
  const totalSections = Array.isArray(report?.sections) ? report.sections.length : 0

  async function onDownloadReport() {
    try {
      await exportReportPdf({ report, payload: jsonData })
      toast.success('PDF report downloaded')
    } catch (error) {
      console.error('PDF export failed:', error)
      toast.error('Failed to generate PDF report')
    }
  }

  return (
    <section ref={resultsRef} className="space-y-4">
      <Card className="border-white/10 bg-slate-900/60">
        <CardContent className="flex flex-wrap items-center justify-between gap-4 p-5">
          <div className="space-y-1">
            <h2 className="text-xl font-semibold">Extracted Results</h2>
            <p className="text-sm text-slate-300">
              Review a clean summary, section-wise findings, and raw response data.
            </p>
            <div className="flex flex-wrap gap-2 pt-1">
              <Badge variant="secondary">Company: {report?.company_name || '-'}</Badge>
              <Badge variant="secondary">Sections: {totalSections}</Badge>
              <Badge variant="secondary">Confidence: {pct(report?.confidence_score)}</Badge>
            </div>
          </div>
          <button
            type="button"
            onClick={onDownloadReport}
            className="inline-flex items-center gap-2 rounded-lg border border-indigo-300/30 bg-indigo-500/15 px-4 py-2 text-sm font-medium text-indigo-100 transition hover:bg-indigo-500/30"
          >
            <Download className="h-4 w-4" />
            Download PDF Report
          </button>
        </CardContent>
      </Card>

      <Tabs defaultValue="report">
        <TabsList>
          <TabsTrigger value="report">Full Report</TabsTrigger>
          <TabsTrigger value="insights">Insights</TabsTrigger>
          <TabsTrigger value="json">Raw JSON</TabsTrigger>
        </TabsList>

        <TabsContent value="report" className="space-y-3">
          <DeckReport report={report} />
        </TabsContent>

        <TabsContent value="insights">
          <Accordion type="multiple" className="space-y-2">
            {safeInsights.map((insight) => (
              <AccordionItem key={insight.label} value={insight.label}>
                <AccordionTrigger>{insight.label}</AccordionTrigger>
                <AccordionContent>{insight.value}</AccordionContent>
              </AccordionItem>
            ))}
          </Accordion>
        </TabsContent>

        <TabsContent value="json">
          <Card>
            <CardTitle className="px-5 pt-5">Structured output</CardTitle>
            <CardContent>
              <pre className="max-h-80 overflow-auto rounded-xl border border-white/10 bg-slate-950/70 p-4 text-xs text-emerald-300">
                {JSON.stringify(jsonData, null, 2)}
              </pre>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </section>
  )
}
