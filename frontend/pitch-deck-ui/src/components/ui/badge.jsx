import { cn } from '../../lib/utils'

export function Badge({ className, ...props }) {
  return (
    <span
      className={cn('inline-flex items-center rounded-md border border-white/15 bg-white/5 px-2.5 py-1 text-xs text-slate-200', className)}
      {...props}
    />
  )
}
