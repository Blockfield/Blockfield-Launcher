import { useState } from "react";
import type { ReactNode } from "react";
import { Folder, Cpu, Coffee, Globe, RefreshCw, LogOut, Save } from "lucide-react";
import { GridBackdrop, TopoBackdrop } from "./Backdrop";
import { GlowPanel } from "./ui-bits";
import { LANGUAGES, useI18n } from "../i18n";
import { OPERATOR_HANDLE } from "../constants";

export function SettingsScreen({ onLogout }: { onLogout: () => void }) {
  const { lang, setLang, t } = useI18n();
  const [dir, setDir] = useState("C:/Users/Operator/AppData/BlockField");
  const [java, setJava] = useState("C:/Program Files/Java/jdk-21/bin/java.exe");
  const [ram, setRam] = useState(8);
  const [autoUpdate, setAutoUpdate] = useState(true);

  return (
    <div className="relative h-full w-full overflow-hidden bg-[#070604]">
      <TopoBackdrop />
      <GridBackdrop intensity={0.4} />

      <div className="relative h-full p-6 flex flex-col overflow-y-auto">
        <div className="flex items-center gap-3 mb-5">
          <span className="shrink-0 text-[10px] tracking-[0.22em] text-[#8E7A5E]">
            {t("settings.configuration")}
          </span>
          <span className="h-px flex-1 bg-[#18130D]" />
          <span className="shrink-0 text-[10px] tracking-[0.22em] text-[#8E7A5E]">
            {t("settings.operator", { handle: OPERATOR_HANDLE })}
          </span>
        </div>

        <div className="flex items-baseline gap-3">
          <h1 className="tracking-[0.06em] text-[34px] leading-none text-neutral-50">
            {t("nav.settings")}
          </h1>
          <span className="text-[10px] tracking-[0.18em] text-[#8E7A5E]">
            {t("settings.preferences")}
          </span>
        </div>

        <GlowPanel glow={false} className="mt-5">
          <Group title={t("settings.runtime")} code="ENV-001">
            <Setting
              icon={<Folder size={14} />}
              label={t("settings.gameDir")}
              hint={t("settings.gameDirHint")}
            >
              <PathInput value={dir} onChange={setDir} browseLabel={t("settings.browse")} />
            </Setting>
            <Setting
              icon={<Coffee size={14} />}
              label={t("settings.java")}
              hint={t("settings.javaHint")}
            >
              <PathInput value={java} onChange={setJava} browseLabel={t("settings.browse")} />
            </Setting>
            <Setting
              icon={<Cpu size={14} />}
              label={t("settings.ram")}
              hint={t("settings.ramHint", { gb: ram })}
            >
              <RamSlider ram={ram} onChange={setRam} />
            </Setting>
          </Group>

          <Group title={t("settings.launcher")} code="LCH-002">
            <Setting
              icon={<Globe size={14} />}
              label={t("settings.language")}
              hint={t("settings.languageHint")}
            >
              <div className="flex flex-wrap gap-1">
                {LANGUAGES.map((l) => (
                  <button
                    type="button"
                    key={l.code}
                    aria-pressed={lang === l.code}
                    onClick={() => setLang(l.code)}
                    className={`h-9 px-3 text-[10px] tracking-[0.14em] border transition-colors ${
                      lang === l.code
                        ? "border-[#F5A524] bg-[#2A2116] text-[#F3E7D0]"
                        : "border-[#2A2116] bg-[#0B0906] text-[#C7AE86] hover:border-[#8A571C]"
                    }`}
                  >
                    {l.label}
                  </button>
                ))}
              </div>
            </Setting>

            <Setting
              icon={<RefreshCw size={14} />}
              label={t("settings.autoUpdate")}
              hint={t("settings.autoUpdateHint")}
            >
              <Toggle
                on={autoUpdate}
                onChange={setAutoUpdate}
                onLabel={t("settings.enabled")}
                offLabel={t("settings.disabled")}
              />
            </Setting>
          </Group>

          <div className="px-6 py-4 flex items-center justify-between border-t border-[#18130D] bg-[#0B0906]">
            <button
              type="button"
              onClick={onLogout}
              className="h-10 px-5 flex items-center gap-3 border border-[#3a2828] bg-[#1a0e0e] text-[#c98b8b] hover:border-[#7a3838] hover:text-[#e0a3a3] transition-colors"
            >
              <LogOut size={13} />
              <span className="text-[11px] tracking-[0.18em]">{t("settings.logout")}</span>
            </button>
            <div className="flex items-center gap-3">
              <button
                type="button"
                className="h-10 px-5 border border-[#2A2116] text-[#C7AE86] hover:text-neutral-200 hover:border-[#3A2C1D] transition-colors text-[11px] tracking-[0.18em]"
              >
                {t("settings.reset")}
              </button>
              <button
                type="button"
                className="h-10 px-6 flex items-center gap-3 border border-[#F5A524]/40 bg-gradient-to-b from-[#2A2116] to-[#11100D] hover:border-[#F5A524] transition-colors"
                style={{
                  boxShadow:
                    "0 0 24px -8px rgba(245,165,36,0.4), inset 0 0 0 1px rgba(245,165,36,0.08)",
                }}
              >
                <Save size={13} className="text-[#F3E7D0]" />
                <span className="text-[11px] tracking-[0.18em] text-[#F3E7D0]">
                  {t("settings.save")}
                </span>
              </button>
            </div>
          </div>
        </GlowPanel>
      </div>
    </div>
  );
}

function Group({
  title,
  code,
  children,
}: {
  title: string;
  code: string;
  children: ReactNode;
}) {
  return (
    <div className="border-b border-[#18130D] last:border-b-0">
      <div className="px-6 pt-4 pb-2 flex items-center gap-3">
        <span className="size-1.5 bg-[#F5A524]" />
        <span className="text-[10px] tracking-[0.18em] text-[#F3E7D0]">{title}</span>
        <span className="h-px flex-1 bg-[#18130D]" />
        <span className="text-[9px] tracking-[0.28em] text-[#5E5040]">{code}</span>
      </div>
      <div className="px-6 pb-1">{children}</div>
    </div>
  );
}

function Setting({
  icon,
  label,
  hint,
  children,
}: {
  icon: ReactNode;
  label: string;
  hint: string;
  children: ReactNode;
}) {
  return (
    <div className="grid grid-cols-[minmax(230px,280px)_minmax(0,1fr)] gap-6 py-3 border-b border-dashed border-[#18130D] last:border-b-0">
      <div className="flex min-w-0 gap-3">
        <div className="mt-0.5 size-7 grid place-items-center border border-[#2A2116] bg-[#0B0906] text-[#F5A524]">
          {icon}
        </div>
        <div className="flex min-w-0 flex-col">
          <span className="text-[11px] tracking-[0.12em] text-neutral-100">{label}</span>
          <span className="text-[11px] leading-snug text-[#8E7A5E] mt-1">{hint}</span>
        </div>
      </div>
      <div className="flex min-w-0 items-center">{children}</div>
    </div>
  );
}

function PathInput({
  value,
  onChange,
  browseLabel,
}: {
  value: string;
  onChange: (v: string) => void;
  browseLabel: string;
}) {
  return (
    <div className="flex w-full">
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="flex-1 h-10 border border-[#2A2116] bg-[#0B0906] px-3 text-[12px] font-mono text-neutral-200 outline-none focus:border-[#F5A524]/60 transition-colors"
      />
      <button
        type="button"
        className="h-10 shrink-0 px-3 border border-l-0 border-[#2A2116] bg-[#11100D] text-[10px] tracking-[0.14em] text-[#C7AE86] hover:text-[#F3E7D0] hover:border-[#8A571C] transition-colors"
      >
        {browseLabel}
      </button>
    </div>
  );
}

function RamSlider({ ram, onChange }: { ram: number; onChange: (v: number) => void }) {
  return (
    <div className="flex items-center gap-4">
      <div className="relative flex-1 h-1.5 bg-[#0B0906] border border-[#2A2116]">
        <div
          className="absolute top-0 left-0 h-full bg-gradient-to-r from-[#8A571C] to-[#F5A524]"
          style={{ width: `${(ram / 16) * 100}%` }}
        />
        <input
          type="range"
          min={2}
          max={16}
          value={ram}
          onChange={(e) => onChange(parseInt(e.target.value))}
          className="absolute inset-0 w-full opacity-0 cursor-pointer"
        />
      </div>
      <span className="text-[12px] tracking-[0.18em] text-[#F3E7D0] w-16 text-right">{ram} GB</span>
    </div>
  );
}

function Toggle({
  on,
  onChange,
  onLabel,
  offLabel,
}: {
  on: boolean;
  onChange: (v: boolean) => void;
  onLabel: string;
  offLabel: string;
}) {
  return (
    <button
      type="button"
      onClick={() => onChange(!on)}
      className={`relative h-7 w-14 border transition-colors ${
        on ? "border-[#F5A524]/60 bg-[#2A2116]" : "border-[#2A2116] bg-[#0B0906]"
      }`}
    >
      <span
        className={`absolute top-0.5 size-5 transition-all ${
          on
            ? "left-[30px] bg-[#F5A524] shadow-[0_0_10px_rgba(245,165,36,0.6)]"
            : "left-0.5 bg-[#3A2C1D]"
        }`}
      />
      <span
        className={`absolute -bottom-5 right-0 text-[9px] tracking-[0.16em] ${
          on ? "text-[#F5A524]" : "text-[#8E7A5E]"
        }`}
      >
        {on ? onLabel : offLabel}
      </span>
    </button>
  );
}
