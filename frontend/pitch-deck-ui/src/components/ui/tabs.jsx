import * as TabsPrimitive from '@radix-ui/react-tabs'
import { cn } from '../../lib/utils'

export function Tabs(props) {
  return <TabsPrimitive.Root {...props} />
}

export function TabsList({ className, ...props }) {
  return (
    <TabsPrimitive.List
      className={cn('grid grid-cols-3 rounded-xl border border-white/10 bg-slate-900/70 p-1', className)}
      {...props}
    />
  )
}

export function TabsTrigger({ className, ...props }) {
  return (
    <TabsPrimitive.Trigger
      className={cn(
        'rounded-lg px-3 py-2 text-sm text-slate-400 transition data-[state=active]:bg-slate-800 data-[state=active]:text-slate-100',
        className,
      )}
      {...props}
    />
  )
}

export function TabsContent({ className, ...props }) {
  return <TabsPrimitive.Content className={cn('mt-4', className)} {...props} />
}
