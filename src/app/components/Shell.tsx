import type { ReactNode } from 'react'
import { Settings, LogOut, Gamepad2, Download, LifeBuoy } from 'lucide-react'
import { useI18n, type TKey } from '../i18n'
import { Logo } from './Logo'
import { StatusDot } from './ui-bits'
import {
  BRAND,
  LAUNCHER_VERSION,
  OPERATOR_HANDLE,
  OPERATOR_INITIALS,
  SERVER_IP,
} from '../constants'
import { contentText, useLauncherContent } from '../../lib/content'

type Screen = 'main' | 'update' | 'settings'

const NAV_ITEMS: Array<{ id: Screen; icon: ReactNode; key: TKey }> = [
  { id: 'main', icon: <Gamepad2 size={13} />, key: 'nav.deploy' },
  { id: 'update', icon: <Download size={13} />, key: 'nav.updates' },
  { id: 'settings', icon: <Settings size={13} />, key: 'nav.settings' },
]

export function Shell({
  active,
  onNavigate,
  onLogout,
  children,
}: {
  active: Screen
  onNavigate: (s: Screen) => void
  onLogout: () => void
  children: ReactNode
}) {
  const { t } = useI18n()
  const content = useLauncherContent()
  const launcherVersion =
    contentText(content, 'launcherVersion', 'launcher_version') ?? LAUNCHER_VERSION
  const serverIp = contentText(content, 'serverIp', 'server_ip') ?? SERVER_IP
  const operatorHandle =
    contentText(content, 'operatorHandle', 'operator_handle') ?? OPERATOR_HANDLE
  const operatorInitials =
    contentText(content, 'operatorInitials', 'operator_initials') ?? OPERATOR_INITIALS
  const operatorRank = contentText(content, 'operatorRank', 'operator_rank') ?? t('shell.rank')
  const supportLabel = contentText(content, 'supportLabel', 'support_label') ?? t('shell.support')
  const networkStatus =
    contentText(content, 'networkStatus', 'network_status') ?? t('shell.network')
  const brand = contentText(content, 'brand') ?? BRAND
  const brandSubtitle = contentText(content, 'brandSubtitle', 'brand_subtitle') ?? 'TACTICAL OPS'

  return (
    <div className="relative h-full w-full bg-[#070604] flex flex-col">
      <header className="h-14 shrink-0 border-b border-[#18130D] bg-[#0B0906] flex items-center justify-between gap-4 px-5">
        <div className="flex min-w-0 items-center gap-6">
          <Logo size={26} withWordmark wordmark={brand} subtitle={brandSubtitle} />
          <nav className="flex items-center gap-1">
            {NAV_ITEMS.map((item) => (
              <NavItem
                key={item.id}
                icon={item.icon}
                label={t(item.key)}
                active={active === item.id}
                onClick={() => onNavigate(item.id)}
              />
            ))}
          </nav>
        </div>

        <div className="flex shrink-0 items-center gap-3">
          <div className="flex items-center gap-3 px-3 h-9 border border-[#2A2116] bg-[#11100D]">
            <div className="relative">
              <div className="size-6 bg-gradient-to-br from-[#8A571C] to-[#2A2116] grid place-items-center text-[10px] text-[#F3E7D0] tracking-widest">
                {operatorInitials}
              </div>
              <span className="absolute -bottom-0.5 -right-0.5 size-1.5 rounded-full bg-[#82D66B] ring-2 ring-[#11100D]" />
            </div>
            <div className="flex flex-col leading-none">
              <span className="text-[11px] tracking-widest text-neutral-100">{operatorHandle}</span>
              <span className="text-[9px] tracking-[0.12em] text-[#8E7A5E] mt-0.5">
                {operatorRank}
              </span>
            </div>
          </div>
          <button
            onClick={onLogout}
            className="h-9 w-9 grid place-items-center border border-[#2A2116] bg-[#11100D] text-[#8E7A5E] hover:text-[#F3E7D0] hover:border-[#8A571C] transition-colors"
            title={t('shell.logout')}
          >
            <LogOut size={13} />
          </button>
        </div>
      </header>

      <div className="flex-1 relative overflow-hidden">{children}</div>

      <footer className="h-8 shrink-0 border-t border-[#18130D] bg-[#0B0906] flex items-center justify-between gap-4 px-5 text-[10px] tracking-[0.16em] text-[#5E5040]">
        <div className="flex shrink-0 items-center gap-3">
          <span>{t('shell.launcherVersion', { v: launcherVersion })}</span>
          <span className="h-3 w-px bg-[#18130D]" />
          <span>{t('shell.ip', { ip: serverIp })}</span>
        </div>
        <div className="flex shrink-0 items-center gap-3">
          <a className="flex items-center gap-1.5 hover:text-[#F5A524] cursor-pointer">
            <LifeBuoy size={11} /> {supportLabel}
          </a>
          <span className="h-3 w-px bg-[#18130D]" />
          <span className="flex items-center gap-1.5">
            <StatusDot />
            {networkStatus}
          </span>
        </div>
      </footer>
    </div>
  )
}

function NavItem({
  icon,
  label,
  active,
  onClick,
}: {
  icon: ReactNode
  label: string
  active: boolean
  onClick: () => void
}) {
  return (
    <button
      onClick={onClick}
      className={`relative h-9 px-3.5 flex items-center gap-2 text-[10px] tracking-[0.28em] transition-colors ${
        active ? 'text-[#F3E7D0]' : 'text-[#8E7A5E] hover:text-neutral-200'
      }`}
    >
      {icon}
      {label}
      {active && <span className="absolute left-2 right-2 -bottom-px h-px bg-[#F5A524]" />}
    </button>
  )
}
