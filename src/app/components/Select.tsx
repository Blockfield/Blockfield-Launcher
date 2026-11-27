import { Check, ChevronDown } from 'lucide-react'
import { useEffect, useId, useRef, useState } from 'react'

export interface SelectOption {
  value: string
  label: string
}

export function Select({
  id: customId,
  name,
  value,
  options,
  ariaLabel,
  listboxLabel,
  disabled = false,
  className = '',
  onChange,
}: {
  id?: string
  name?: string
  value: string
  options: SelectOption[]
  ariaLabel: string
  listboxLabel?: string
  disabled?: boolean
  className?: string
  onChange: (value: string) => void
}) {
  const generatedId = useId()
  const id = customId ?? generatedId
  const root = useRef<HTMLDivElement>(null)
  const trigger = useRef<HTMLButtonElement>(null)
  const search = useRef({ text: '', time: 0 })
  const [open, setOpen] = useState(false)
  const [activeValue, setActiveValue] = useState(value)

  const selected = options.find((opt) => opt.value === value) ?? options[0]
  const activeIndex = Math.max(
    0,
    options.findIndex((opt) => opt.value === activeValue),
  )

  useEffect(() => {
    if (!open) return
    const closeOutside = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false)
    }
    document.addEventListener('pointerdown', closeOutside)
    return () => document.removeEventListener('pointerdown', closeOutside)
  }, [open])

  useEffect(() => {
    if (open) {
      document.getElementById(`${id}-${activeIndex}`)?.scrollIntoView({ block: 'nearest' })
    }
  }, [open, activeIndex, id])

  const choose = (option: SelectOption) => {
    onChange(option.value)
    setOpen(false)
    trigger.current?.focus()
  }

  return (
    <div
      ref={root}
      className={`relative ${className}`}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) setOpen(false)
      }}
    >
      <button
        ref={trigger}
        id={id}
        name={name}
        type="button"
        role="combobox"
        aria-label={ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={`${id}-listbox`}
        aria-activedescendant={open ? `${id}-${activeIndex}` : undefined}
        disabled={disabled}
        onClick={() => {
          if (disabled) return
          setActiveValue(value)
          setOpen(!open)
        }}
        onKeyDown={(event) => {
          if (disabled) return
          if (event.key === 'Tab') {
            setOpen(false)
            return
          }
          if (event.key === 'Escape') {
            event.preventDefault()
            setOpen(false)
            return
          }
          if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
            event.preventDefault()
            const index = open
              ? activeIndex
              : Math.max(
                  0,
                  options.findIndex((opt) => opt.value === value),
                )
            const next =
              event.key === 'Home'
                ? 0
                : event.key === 'End'
                  ? options.length - 1
                  : Math.max(
                      0,
                      Math.min(options.length - 1, index + (event.key === 'ArrowDown' ? 1 : -1)),
                    )
            const target = options[next] ?? selected
            if (target) setActiveValue(target.value)
            setOpen(true)
          } else if ((event.key === 'Enter' || event.key === ' ') && open) {
            event.preventDefault()
            const target = options[activeIndex] ?? selected
            if (target) choose(target)
          } else if (
            event.key.length === 1 &&
            event.key !== ' ' &&
            !event.ctrlKey &&
            !event.metaKey &&
            !event.altKey
          ) {
            event.preventDefault()
            const now = Date.now()
            search.current.text =
              (now - search.current.time < 700 ? search.current.text : '') +
              event.key.toLocaleLowerCase()
            search.current.time = now
            const match = options.find((opt) =>
              opt.label.toLocaleLowerCase().startsWith(search.current.text),
            )
            if (match) {
              setActiveValue(match.value)
              setOpen(true)
            }
          }
        }}
        className={`flex h-8 w-full min-w-0 items-center justify-between gap-3 border bg-[#11100D] px-3 text-left text-[14px] text-[#F3E7D0] transition-colors hover:border-[#8A571C] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[#F5A524] disabled:cursor-not-allowed disabled:opacity-50 ${
          open ? 'border-[#8A571C]' : 'border-[#2A2116]'
        }`}
      >
        <span className="truncate">{selected?.label ?? ''}</span>
        <ChevronDown
          aria-hidden="true"
          size={14}
          className={`shrink-0 text-[#C7AE86] transition-transform ${open ? 'rotate-180' : ''}`}
        />
      </button>
      <ul
        id={`${id}-listbox`}
        role="listbox"
        aria-label={listboxLabel ?? ariaLabel}
        hidden={!open}
        className="absolute inset-x-0 top-full z-50 mt-1 max-h-48 overflow-y-auto overscroll-contain border border-[#8A571C] bg-[#11100D] p-1 [scrollbar-color:#8A571C_#11100D] [scrollbar-width:thin]"
      >
        {options.map((opt, index) => {
          const isSelected = opt.value === value
          return (
            <li
              key={opt.value}
              id={`${id}-${index}`}
              role="option"
              aria-selected={isSelected}
              onPointerMove={() => setActiveValue(opt.value)}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => choose(opt)}
              className={`flex min-h-9 cursor-pointer items-center justify-between gap-3 px-2 py-2 text-[14px] ${
                index === activeIndex ? 'bg-[#2A2116] text-[#F3E7D0]' : 'text-[#C7AE86]'
              } ${isSelected ? 'text-[#F5A524]' : ''}`}
            >
              <span className="min-w-0 flex-1 truncate">{opt.label}</span>
              {isSelected && (
                <Check aria-hidden="true" size={14} className="shrink-0 text-[#F5A524]" />
              )}
            </li>
          )
        })}
      </ul>
    </div>
  )
}
