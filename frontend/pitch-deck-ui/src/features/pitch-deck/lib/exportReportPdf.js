function toPercent(value) {
  if (typeof value !== 'number' || Number.isNaN(value)) return '-'
  return `${(value * 100).toFixed(1)}%`
}

function pushWrappedLine(doc, text, x, y, maxWidth, lineHeight) {
  const safeText = String(text ?? '-')
  const lines = doc.splitTextToSize(safeText, maxWidth)
  doc.text(lines, x, y)
  return y + lines.length * lineHeight
}

export async function exportReportPdf({ report, payload }) {
  const { jsPDF } = await import('jspdf')
  const doc = new jsPDF({ unit: 'pt', format: 'a4' })
  const pageWidth = doc.internal.pageSize.getWidth()
  const pageHeight = doc.internal.pageSize.getHeight()
  const margin = 48
  const contentWidth = pageWidth - margin * 2
  const lineHeight = 16
  let y = margin

  const ensurePageSpace = (needed = 64) => {
    if (y + needed < pageHeight - margin) return
    doc.addPage()
    y = margin
  }

  doc.setFont('helvetica', 'bold')
  doc.setFontSize(18)
  doc.text('Pitch Deck Analysis Report', margin, y)
  y += 24

  doc.setFont('helvetica', 'normal')
  doc.setFontSize(11)
  y = pushWrappedLine(doc, `Generated: ${new Date().toLocaleString()}`, margin, y, contentWidth, lineHeight)
  y = pushWrappedLine(doc, `Request ID: ${payload?.request_id || '-'}`, margin, y, contentWidth, lineHeight)
  y = pushWrappedLine(doc, `Status: ${payload?.status || '-'}`, margin, y, contentWidth, lineHeight)
  y += 8

  if (!report) {
    doc.setFont('helvetica', 'italic')
    doc.text('No structured report returned by backend.', margin, y)
    doc.save('pitch-deck-analysis-report.pdf')
    return
  }

  ensurePageSpace(100)
  doc.setFont('helvetica', 'bold')
  doc.setFontSize(14)
  doc.text('Overview', margin, y)
  y += 18

  doc.setFont('helvetica', 'normal')
  doc.setFontSize(11)
  y = pushWrappedLine(doc, `Company: ${report.company_name || '-'}`, margin, y, contentWidth, lineHeight)
  y = pushWrappedLine(doc, `Website: ${report.company_website || '-'}`, margin, y, contentWidth, lineHeight)
  y = pushWrappedLine(doc, `File: ${report.filename || '-'}`, margin, y, contentWidth, lineHeight)
  y = pushWrappedLine(doc, `Confidence score: ${toPercent(report.confidence_score)}`, margin, y, contentWidth, lineHeight)
  y += 8

  if (report.overall_summary) {
    ensurePageSpace(120)
    doc.setFont('helvetica', 'bold')
    doc.setFontSize(14)
    doc.text('Overall Summary', margin, y)
    y += 18
    doc.setFont('helvetica', 'normal')
    doc.setFontSize(11)
    y = pushWrappedLine(doc, report.overall_summary, margin, y, contentWidth, lineHeight)
    y += 10
  }

  const sections = Array.isArray(report.sections) ? report.sections : []
  sections.forEach((section, idx) => {
    ensurePageSpace(140)
    doc.setFont('helvetica', 'bold')
    doc.setFontSize(13)
    doc.text(`${idx + 1}. ${section?.section_name || 'Section'}`, margin, y)
    y += 18

    doc.setFont('helvetica', 'normal')
    doc.setFontSize(11)
    y = pushWrappedLine(
      doc,
      `Confidence: ${toPercent(section?.confidence)} | Validation: ${toPercent(section?.validation?.score)}`,
      margin,
      y,
      contentWidth,
      lineHeight
    )

    if (section?.summary) {
      y = pushWrappedLine(doc, `Summary: ${section.summary}`, margin, y, contentWidth, lineHeight)
    }

    const signals = Array.isArray(section?.signals) ? section.signals : []
    if (signals.length > 0) {
      y = pushWrappedLine(doc, `Signals (${signals.length}):`, margin, y, contentWidth, lineHeight)
      signals.slice(0, 6).forEach((signal) => {
        ensurePageSpace(50)
        y = pushWrappedLine(
          doc,
          `- ${signal?.signal_type || 'Signal'}: ${signal?.description || ''}`,
          margin + 12,
          y,
          contentWidth - 12,
          lineHeight
        )
      })
    }

    const flags = Array.isArray(section?.red_flags) ? section.red_flags : []
    if (flags.length > 0) {
      y = pushWrappedLine(doc, `Red flags (${flags.length}):`, margin, y, contentWidth, lineHeight)
      flags.slice(0, 6).forEach((flag) => {
        ensurePageSpace(50)
        y = pushWrappedLine(
          doc,
          `- ${flag?.flag_type || 'Flag'} [${flag?.severity || '-'}]: ${flag?.description || ''}`,
          margin + 12,
          y,
          contentWidth - 12,
          lineHeight
        )
      })
    }

    y += 10
  })

  doc.save(`pitch-deck-report-${report.deck_id || 'analysis'}.pdf`)
}
