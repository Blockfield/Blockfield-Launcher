import { useLauncherUpdate } from '../../lib/launcher-update'
import type { ReactNode } from 'react'
import { Settings, Gamepad2, Download, LifeBuoy } from 'lucide-react'
import { useI18n, type TKey } from '../i18n'
import { Logo } from './Logo'
import { StatusDot } from './ui-bits'
import { BRAND, RELEASES_REPO_URL, SERVER_IP } from '../constants'
import { contentText, localizedContentText, useLauncherContent } from '../../lib/content'
import { useLauncherVersion } from '../../lib/version'
import { openExternalUrl } from '../../lib/api'

type Screen = 'main' | 'update' | 'settings'

const NAV_ITEMS: Array<{ id: Screen; icon: ReactNode; key: TKey }> = [
  { id: 'main', icon: <Gamepad2 size={13} />, key: 'nav.deploy' },
  { id: 'update', icon: <Download size={13} />, key: 'nav.updates' },
  { id: 'settings', icon: <Settings size={13} />, key: 'nav.settings' },
]

export function Shell({
  active,
  user,
  onNavigate,
  onLauncherUpdate,
  children,
}: {
  active: Screen
  user: { username: string; role: string }
  onNavigate: (s: Screen) => void
  onLauncherUpdate: () => void
  children: ReactNode
}) {
  const { lang, t } = useI18n()
  const content = useLauncherContent()
  const launcherVersion = useLauncherVersion()
  const update = useLauncherUpdate()
  const updateLabel = update.installed
    ? 'Перезапустить'
    : update.phase === 'available'
      ? `Доступна ${update.update?.version}`
      : update.phase === 'downloading'
        ? `Загрузка ${update.total ? Math.min(100, Math.round((update.downloaded / update.total) * 100)) + '%' : '…'}`
        : update.phase === 'checking'
          ? 'Проверка…'
          : update.phase === 'verifying'
            ? 'Проверка подписи…'
            : update.phase === 'installing'
              ? 'Установка…'
              : update.phase === 'error'
                ? 'Ошибка обновления'
                : null
  const serverIp = contentText(content, 'serverIp', 'server_ip') ?? SERVER_IP
  const operatorHandle = user.username
  const operatorInitials = user.username.slice(0, 2).toUpperCase()
  const operatorRank = user.role.toUpperCase()
  const supportLabel =
    localizedContentText(content, lang, 'content.supportLabel', 'supportLabel', 'support_label') ??
    t('shell.support')
  const supportUrl = contentText(content, 'supportUrl', 'support_url') ?? RELEASES_REPO_URL
  const networkStatus = t('shell.offlineMode')
  const brand = contentText(content, 'brand') ?? BRAND
  const brandSubtitle =
    localizedContentText(
      content,
      lang,
      'content.brandSubtitle',
      'brandSubtitle',
      'brand_subtitle',
    ) ?? 'TACTICAL OPS'

  return (
    <div className="relative h-full w-full bg-[#070604] flex flex-col">
      <header className="shell-header h-14 shrink-0 border-b border-[#18130D] bg-[#0B0906] flex items-center justify-between gap-4 px-5">
        <div className="flex min-w-0 items-center gap-6">
          <button
            type="button"
            onClick={() => onNavigate('main')}
            aria-label={`${brand} — ${t('nav.deploy')}`}
            className="shrink-0 text-left cursor-pointer focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-[#F5A524]"
          >
            <Logo size={26} withWordmark wordmark={brand} subtitle={brandSubtitle} />
          </button>
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
        </div>
      </header>

      <div className="flex-1 min-h-0 relative overflow-hidden">{children}</div>

      <footer className="shell-footer h-8 shrink-0 border-t border-[#18130D] bg-[#0B0906] flex items-center justify-between gap-4 px-5 text-[10px] tracking-[0.16em] text-[#5E5040]">
        <div className="flex shrink-0 items-center gap-3">
          <button
            onClick={onLauncherUpdate}
            aria-label="Открыть обновление лаунчера"
            className="flex min-w-0 items-center gap-2 hover:text-[#F3E7D0] focus-visible:outline-2 focus-visible:outline-[#F5A524]"
          >
            <span>{t('shell.launcherVersion', { v: launcherVersion })}</span>
            {updateLabel && <span className="text-[#F5A524] tracking-normal">· {updateLabel}</span>}
          </button>
          <span className="h-3 w-px bg-[#18130D]" />
          <span>{t('shell.ip', { ip: serverIp })}</span>
        </div>
        <div className="flex shrink-0 items-center gap-3">
          <a
            href={supportUrl}
            onClick={(e) => {
              e.preventDefault()
              void openExternalUrl(supportUrl)
            }}
            target="_blank"
            rel="noreferrer"
            aria-label="Поддержка Blockfield"
            className="flex items-center gap-1.5 hover:text-[#F5A524] cursor-pointer transition-colors focus-visible:outline-2 focus-visible:outline-[#F5A524]"
          >
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
