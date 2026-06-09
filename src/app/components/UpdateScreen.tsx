import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { Play, Pause, Download, HardDrive, FileBox, Zap } from "lucide-react";
import { GridBackdrop, TopoBackdrop } from "./Backdrop";
import { GlowPanel, OperationBar, SectionHeader, StatusDot } from "./ui-bits";
import { useI18n, type TKey, type TFunction } from "../i18n";
import { MODPACK_VERSION, OPERATION_NAME } from "../constants";

const FILES = [
  "battlefield-core-0.1.42.jar",
  "vehicle-physics-engine.dll",
  "map_blackridge_v3.dat",
  "shaders/iron-front.pak",
  "assets/audio/ambient_warzone.ogg",
  "anti-cheat/kernel-driver.sys",
];

const TOTAL_MB = 2148;
const TICK_MS = 220;
const PROGRESS_STEP = 0.6;
const PROGRESS_RESET = 12;

const LOG_LINES: Array<{ ts: string; tone: "ok" | "info" | "dim"; key: TKey }> = [
  { ts: "14:02:11", tone: "ok", key: "update.log.handshake" },
  { ts: "14:02:12", tone: "ok", key: "update.log.manifest" },
  { ts: "14:02:13", tone: "info", key: "update.log.phase1" },
  { ts: "14:02:18", tone: "info", key: "update.log.phase2" },
];

type StepStatus = "done" | "active" | "pending";
const STEPS: Array<{ label: TKey; status: StepStatus }> = [
  { label: "update.step.verify", status: "done" },
  { label: "update.step.prune", status: "done" },
  { label: "update.step.download", status: "active" },
  { label: "update.step.integrity", status: "pending" },
  { label: "update.step.finalize", status: "pending" },
];

export function UpdateScreen() {
  const { t } = useI18n();
  const [progress, setProgress] = useState(34);
  const [fileIdx, setFileIdx] = useState(2);

  useEffect(() => {
    const id = setInterval(() => {
      setProgress((p) => (p + PROGRESS_STEP >= 100 ? PROGRESS_RESET : p + PROGRESS_STEP));
      setFileIdx((i) => (Math.random() > 0.85 ? (i + 1) % FILES.length : i));
    }, TICK_MS);
    return () => clearInterval(id);
  }, []);

  const downloadedMB = Math.round((progress / 100) * TOTAL_MB);

  return (
    <div className="relative h-full w-full overflow-hidden bg-[#070604]">
      <TopoBackdrop />
      <GridBackdrop intensity={0.5} />

      <div className="relative h-full p-5 flex flex-col min-h-0">
        <OperationBar
          label={t("update.packageSync")}
          status={t("update.updating")}
          statusColor="#F5A524"
        />

        <div className="flex items-start justify-between gap-6 min-w-0">
          <div>
            <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1 mb-1.5">
              <h1 className="tracking-[0.06em] text-[26px] leading-none text-neutral-50">
                {OPERATION_NAME}
              </h1>
              <span className="text-[10px] tracking-[0.18em] text-[#8E7A5E]">
                {t("update.patch", { v: MODPACK_VERSION })}
              </span>
            </div>
            <p className="text-[12px] leading-snug text-[#C7AE86] max-w-[520px]">
              {t("update.description")}
            </p>
          </div>
          <div className="shrink-0 border border-[#2A2116] bg-[#0B0906] px-4 py-3 w-[240px]">
            <div className="text-[10px] tracking-[0.16em] text-[#8E7A5E]">{t("update.mirror")}</div>
            <div className="mt-2 tracking-[0.18em] text-[13px] text-neutral-100">CDN-FRA-02</div>
            <div className="mt-2 text-[10px] tracking-[0.22em] text-[#82D66B]">▸ 18.4 MB/s</div>
          </div>
        </div>

        <GlowPanel className="p-4">
          <div className="flex items-center justify-between mb-3">
            <div className="flex items-center gap-4">
              <LockedButton lockedLabel={t("update.locked")} subLabel={t("update.inProgress")} />
              <button className="h-[40px] w-[40px] grid place-items-center border border-[#2A2116] hover:border-[#8A571C] text-[#C7AE86] hover:text-[#F3E7D0] transition-colors">
                <Pause size={14} />
              </button>
            </div>

            <div className="text-right">
              <div className="text-[10px] tracking-[0.18em] text-[#8E7A5E]">
                {t("update.completion")}
              </div>
              <div className="tracking-[0.04em] text-[#F3E7D0] leading-none mt-1.5 text-[44px]">
                {progress.toFixed(1)}
                <span className="text-[#8E7A5E] text-[20px]">%</span>
              </div>
            </div>
          </div>

          <ProgressBar progress={progress} />

          <div className="mt-4 grid grid-cols-4 gap-px bg-[#18130D] border border-[#2A2116] min-w-0">
            <SubStat
              icon={<FileBox size={13} />}
              label={t("update.currentFile")}
              value={FILES[fileIdx]}
              mono
            />
            <SubStat
              icon={<HardDrive size={13} />}
              label={t("update.transferred")}
              value={`${downloadedMB} MB / ${TOTAL_MB} MB`}
            />
            <SubStat icon={<Download size={13} />} label={t("update.remaining")} value={t("update.eta")} />
            <SubStat
              icon={<Zap size={13} />}
              label={t("update.throughput")}
              value="18.4 MB/s"
              accent
            />
          </div>

          <div className="mt-3 flex items-start justify-between gap-4 text-[10px] tracking-[0.16em]">
            <div className="flex min-w-0 items-start gap-2 text-[#F5A524]">
              <StatusDot color="#F5A524" pulse />
              <span className="leading-snug">{t("update.status")}</span>
            </div>
            <span className="shrink-0 text-right text-[#8E7A5E]">{t("update.integrity")}</span>
          </div>
        </GlowPanel>

        <div className="mt-3 min-h-0 flex-1 grid grid-cols-[minmax(0,1.35fr)_minmax(300px,1fr)] gap-px bg-[#18130D] border border-[#2A2116] overflow-hidden">
          <div className="bg-[#0B0906] p-4 min-h-0 overflow-hidden">
            <SectionHeader label={t("update.opLog")} code="SYNC-PHASE-2" />
            <div className="mt-3 font-mono text-[11px] leading-relaxed text-[#C7AE86] space-y-1">
              {LOG_LINES.map((line) => (
                <LogLine key={line.ts} ts={line.ts} tone={line.tone} msg={t(line.key)} />
              ))}
              <LogLine ts="14:02:21" tone="dim" msg={`▸ ${FILES[fileIdx]}`} />
              <LogLine ts="14:02:22" tone="dim" msg="▸ assets/vehicles/t72_textures.pak" />
              <LogLine ts="14:02:24" tone="dim" msg="▸ maps/blackridge/heightmap.dat" />
            </div>
          </div>
          <div className="bg-[#0B0906] p-4 flex flex-col min-h-0">
            <SectionHeader label={t("update.steps")} code="SEQ" />
            <div className="mt-3 flex flex-col gap-2 flex-1">
              {STEPS.map((step) => (
                <Step key={step.label} label={t(step.label)} status={step.status} t={t} />
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function ProgressBar({ progress }: { progress: number }) {
  return (
    <div className="relative h-2.5 bg-[#0B0906] border border-[#2A2116] overflow-hidden">
      <div
        className="h-full bg-gradient-to-r from-[#8A571C] via-[#F5A524] to-[#FFC861] transition-all duration-200"
        style={{ width: `${progress}%`, boxShadow: "0 0 16px rgba(245,165,36,0.55)" }}
      />
      <div className="absolute inset-0 pointer-events-none flex">
        {Array.from({ length: 40 }).map((_, i) => (
          <span key={i} className="flex-1 border-r border-[#070604]/60" />
        ))}
      </div>
    </div>
  );
}

function LockedButton({ lockedLabel, subLabel }: { lockedLabel: string; subLabel: string }) {
  return (
    <button
      disabled
      className="relative h-[64px] w-[230px] shrink-0 overflow-hidden border border-[#2A2116] bg-[#0B0906] opacity-60 cursor-not-allowed"
    >
      <span className="absolute top-0 left-0 w-3 h-3 border-l border-t border-[#3A2C1D]" />
      <span className="absolute top-0 right-0 w-3 h-3 border-r border-t border-[#3A2C1D]" />
      <span className="absolute bottom-0 left-0 w-3 h-3 border-l border-b border-[#3A2C1D]" />
      <span className="absolute bottom-0 right-0 w-3 h-3 border-r border-b border-[#3A2C1D]" />
      <span className="h-full flex items-center justify-center gap-4">
        <Play size={18} className="text-[#5E5040]" />
        <span className="flex flex-col items-start leading-none">
          <span className="tracking-[0.2em] text-[17px] text-[#8E7A5E]">{lockedLabel}</span>
          <span className="tracking-[0.14em] text-[9px] text-[#5E5040] mt-1 text-left">{subLabel}</span>
        </span>
      </span>
    </button>
  );
}

function SubStat({
  icon,
  label,
  value,
  mono,
  accent,
}: {
  icon: ReactNode;
  label: string;
  value: string;
  mono?: boolean;
  accent?: boolean;
}) {
  return (
    <div className="bg-[#0B0906] p-4 min-w-0">
      <div className="flex items-center gap-2 text-[9px] tracking-[0.14em] text-[#8E7A5E]">
        {icon} {label}
      </div>
      <div
        className={`mt-2 text-[12px] tracking-[0.12em] truncate ${
          accent ? "text-[#82D66B]" : "text-neutral-100"
        } ${mono ? "font-mono" : ""}`}
      >
        {value}
      </div>
    </div>
  );
}

const LOG_TONE: Record<"ok" | "info" | "dim", string> = {
  ok: "text-[#82D66B]",
  info: "text-[#F5A524]",
  dim: "text-[#8E7A5E]",
};

function LogLine({ ts, tone, msg }: { ts: string; tone: "ok" | "info" | "dim"; msg: string }) {
  return (
    <div className="flex gap-4 min-w-0">
      <span className="text-[#5E5040]">[{ts}]</span>
      <span className={LOG_TONE[tone]}>{msg}</span>
    </div>
  );
}

const STEP_DOT: Record<StepStatus, string> = {
  done: "bg-[#82D66B] shadow-[0_0_6px_rgba(130,214,107,0.5)]",
  active: "bg-[#F5A524] animate-pulse shadow-[0_0_10px_rgba(245,165,36,0.7)]",
  pending: "bg-[#3A2C1D]",
};
const STEP_TEXT: Record<StepStatus, string> = {
  done: "text-[#C7AE86]",
  active: "text-[#F3E7D0]",
  pending: "text-[#8E7A5E]",
};
const STEP_TAG: Record<StepStatus, TKey> = {
  done: "update.step.done",
  active: "update.step.running",
  pending: "update.step.queued",
};

function Step({
  label,
  status,
  t,
}: {
  label: string;
  status: StepStatus;
  t: TFunction;
}) {
  return (
    <div className="flex items-start justify-between gap-3 border-b border-dashed border-[#18130D] pb-2 last:border-0">
      <div className="flex items-center gap-3">
        <span className={`size-2 rounded-full ${STEP_DOT[status]}`} />
        <span className={`text-[11px] tracking-[0.12em] ${STEP_TEXT[status]}`}>{label}</span>
      </div>
      <span className="shrink-0 text-[9px] tracking-[0.16em] text-[#8E7A5E]">{t(STEP_TAG[status])}</span>
    </div>
  );
}
