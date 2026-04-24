import * as AccordionPrimitive from '@radix-ui/react-accordion'
import { ChevronDown } from 'lucide-react'
import { cn } from '../../lib/utils'

export function Accordion(props) {
  return <AccordionPrimitive.Root {...props} />
}

export function AccordionItem({ className, ...props }) {
  return (
    <AccordionPrimitive.Item
      className={cn('rounded-xl border border-white/10 bg-slate-900/65 px-4', className)}
      {...props}
    />
  )
}

export function AccordionTrigger({ className, children, ...props }) {
  return (
    <AccordionPrimitive.Header>
      <AccordionPrimitive.Trigger
        className={cn('flex w-full items-center justify-between py-3 text-left text-sm text-slate-100', className)}
        {...props}
      >
        {children}
        <ChevronDown className="h-4 w-4 text-slate-400" />
      </AccordionPrimitive.Trigger>
    </AccordionPrimitive.Header>
  )
}

export function AccordionContent({ className, ...props }) {
  return <AccordionPrimitive.Content className={cn('pb-4 text-sm text-slate-300', className)} {...props} />
}
