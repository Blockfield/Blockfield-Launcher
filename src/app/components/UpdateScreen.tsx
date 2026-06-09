import { useEffect, useState } from "react";
import { Play, Pause, Download, HardDrive, FileBox, Zap } from "lucide-react";
import { GridBackdrop, TopoBackdrop } from "./Backdrop";

const FILES = [
  "battlefield-core-0.1.42.jar",
  "vehicle-physics-engine.dll",
  "map_blackridge_v3.dat",
  "shaders/iron-front.pak",
  "assets/audio/ambient_warzone.ogg",
  "anti-cheat/kernel-driver.sys",
];

export function UpdateScreen() {
  const [progress, setProgress] = useState(34);
  const [fileIdx, setFileIdx] = useState(2);

  useEffect(() => {
    const id = setInterval(() => {
      setProgress((p) => {
        const next = p + 0.6;
        if (next >= 100) return 12;
        return next;
      });
      setFileIdx((i) => (Math.random() > 0.85 ? (i + 1) % FILES.length : i));
    }, 220);
    return () => clearInterval(id);
  }, []);

  const totalMB = 2148;
  const downloadedMB = Math.round((progress / 100) * totalMB);

  return (
    <div className="relative h-full w-full overflow-hidden bg-[#070604]">
      <TopoBackdrop />
      <GridBackdrop intensity={0.5} />

      <div className="relative h-full p-8 flex flex-col">
        <div className="flex items-center gap-3 mb-5">
          <span className="text-[10px] tracking-[0.4em] text-[#8E7A5E]">
            PACKAGE SYNC
          </span>
          <span className="h-px flex-1 bg-[#18130D]" />
          <span className="flex items-center gap-2 text-[10px] tracking-[0.4em] text-[#F5A524]">
            <span className="size-1.5 rounded-full bg-[#F5A524] animate-pulse shadow-[0_0_6px_rgba(245,165,36,0.7)]" />
            UPDATING MODPACK
          </span>
        </div>

        {/* Header */}
        <div className="flex items-start justify-between gap-6">
          <div>
            <div className="flex items-baseline gap-3 mb-2">
              <h1 className="tracking-[0.06em] text-[34px] leading-none text-neutral-50">
                IRON FRONT
              </h1>
              <span className="text-[10px] tracking-[0.32em] text-[#8E7A5E]">
                / PATCH 0.1.42
              </span>
            </div>
            <p className="text-[13px] leading-relaxed text-[#C7AE86] max-w-[520px]">
              Synchronizing modpack assets with the primary deployment server.
              Do not close the launcher until the operation completes.
            </p>
          </div>
          <div className="border border-[#2A2116] bg-[#0B0906] px-4 py-3 min-w-[220px]">
            <div className="text-[10px] tracking-[0.3em] text-[#8E7A5E]">
              MIRROR
            </div>
            <div className="mt-2 tracking-[0.18em] text-[13px] text-neutral-100">
              CDN-FRA-02
            </div>
            <div className="mt-2 text-[10px] tracking-[0.22em] text-[#82D66B]">
              ▸ 18.4 MB/s
            </div>
          </div>
        </div>

        {/* Main update panel */}
        <div className="mt-8 relative">
          <div className="absolute -inset-px border border-[#F5A524]/20 pointer-events-none" />
          <div className="relative border border-[#2A2116] bg-gradient-to-br from-[#11100D] via-[#11100D] to-[#0B0906] p-7">
            <div className="flex items-center justify-between mb-5">
              <div className="flex items-center gap-4">
                <button
                  disabled
                  className="relative h-[88px] w-[260px] overflow-hidden border border-[#2A2116] bg-[#0B0906] opacity-60 cursor-not-allowed"
                >
                  <span className="absolute top-0 left-0 w-3 h-3 border-l border-t border-[#3A2C1D]" />
                  <span className="absolute top-0 right-0 w-3 h-3 border-r border-t border-[#3A2C1D]" />
                  <span className="absolute bottom-0 left-0 w-3 h-3 border-l border-b border-[#3A2C1D]" />
                  <span className="absolute bottom-0 right-0 w-3 h-3 border-r border-b border-[#3A2C1D]" />
                  <span className="h-full flex items-center justify-center gap-4">
                    <Play size={22} className="text-[#5E5040]" />
                    <span className="flex flex-col items-start leading-none">
                      <span className="tracking-[0.4em] text-[20px] text-[#8E7A5E]">
                        LOCKED
                      </span>
                      <span className="tracking-[0.32em] text-[10px] text-[#5E5040] mt-1.5">
                        UPDATE IN PROGRESS
                      </span>
                    </span>
                  </span>
                </button>
                <button className="h-[44px] w-[44px] grid place-items-center border border-[#2A2116] hover:border-[#8A571C] text-[#C7AE86] hover:text-[#F3E7D0] transition-colors">
                  <Pause size={14} />
                </button>
              </div>

              <div className="text-right">
                <div className="text-[10px] tracking-[0.32em] text-[#8E7A5E]">
                  COMPLETION
                </div>
                <div className="tracking-[0.04em] text-[#F3E7D0] leading-none mt-2 text-[56px]">
                  {progress.toFixed(1)}
                  <span className="text-[#8E7A5E] text-[24px]">%</span>
                </div>
              </div>
            </div>

            {/* Progress bar */}
            <div className="relative h-2.5 bg-[#0B0906] border border-[#2A2116] overflow-hidden">
              <div
                className="h-full bg-gradient-to-r from-[#8A571C] via-[#F5A524] to-[#FFC861] transition-all duration-200"
                style={{
                  width: `${progress}%`,
                  boxShadow: "0 0 16px rgba(245,165,36,0.55)",
                }}
              />
              <div className="absolute inset-0 pointer-events-none flex">
                {Array.from({ length: 40 }).map((_, i) => (
                  <span
                    key={i}
                    className="flex-1 border-r border-[#070604]/60"
                  />
                ))}
              </div>
            </div>

            {/* Sub stats */}
            <div className="mt-5 grid grid-cols-4 gap-px bg-[#18130D] border border-[#2A2116]">
              <SubStat
                icon={<FileBox size={13} />}
                label="CURRENT FILE"
                value={FILES[fileIdx]}
                mono
              />
              <SubStat
                icon={<HardDrive size={13} />}
                label="TRANSFERRED"
                value={`${downloadedMB} MB / ${totalMB} MB`}
              />
              <SubStat
                icon={<Download size={13} />}
                label="REMAINING"
                value="~ 1m 42s"
              />
              <SubStat
                icon={<Zap size={13} />}
                label="THROUGHPUT"
                value="18.4 MB/s"
                tone="cyan"
              />
            </div>

            <div className="mt-5 flex items-center justify-between text-[10px] tracking-[0.28em]">
              <div className="flex items-center gap-2 text-[#F5A524]">
                <span className="size-1.5 rounded-full bg-[#F5A524] animate-pulse shadow-[0_0_6px_rgba(245,165,36,0.7)]" />
                STATUS · UPDATING MODPACK (PHASE 2 OF 3)
              </div>
              <span className="text-[#8E7A5E]">
                INTEGRITY · SHA256 VERIFIED
              </span>
            </div>
          </div>
        </div>

        {/* Log / steps */}
        <div className="mt-6 flex-1 grid grid-cols-[1.4fr_1fr] gap-px bg-[#18130D] border border-[#2A2116]">
          <div className="bg-[#0B0906] p-6">
            <SectionHeader label="OPERATION LOG" code="SYNC-PHASE-2" />
            <div className="mt-4 font-mono text-[11px] leading-relaxed text-[#C7AE86] space-y-1">
              <LogLine ts="14:02:11" tone="ok" msg="Handshake established with CDN-FRA-02" />
              <LogLine ts="14:02:12" tone="ok" msg="Manifest verified · 142 files queued" />
              <LogLine ts="14:02:13" tone="info" msg="Phase 1/3 — pruning stale assets … done" />
              <LogLine ts="14:02:18" tone="info" msg="Phase 2/3 — downloading modpack payload" />
              <LogLine ts="14:02:21" tone="dim" msg={`▸ ${FILES[fileIdx]}`} />
              <LogLine ts="14:02:22" tone="dim" msg="▸ assets/vehicles/t72_textures.pak" />
              <LogLine ts="14:02:24" tone="dim" msg="▸ maps/blackridge/heightmap.dat" />
            </div>
          </div>
          <div className="bg-[#0B0906] p-6 flex flex-col">
            <SectionHeader label="DEPLOYMENT STEPS" code="SEQ" />
            <div className="mt-4 flex flex-col gap-3 flex-1">
              <Step label="VERIFY MANIFEST" status="done" />
              <Step label="PRUNE STALE FILES" status="done" />
              <Step label="DOWNLOAD PAYLOAD" status="active" />
              <Step label="INTEGRITY CHECK" status="pending" />
              <Step label="FINALIZE INSTALL" status="pending" />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function SectionHeader({ label, code }: { label: string; code: string }) {
  return (
    <div className="flex items-center gap-3">
      <span className="size-1.5 bg-[#F5A524]" />
      <span className="text-[10px] tracking-[0.32em] text-[#F3E7D0]">
        {label}
      </span>
      <span className="h-px flex-1 bg-[#18130D]" />
      <span className="text-[9px] tracking-[0.28em] text-[#5E5040]">
        {code}
      </span>
    </div>
  );
}

function SubStat({
  icon,
  label,
  value,
  mono,
  tone,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
  mono?: boolean;
  tone?: "cyan";
}) {
  return (
    <div className="bg-[#0B0906] p-4">
      <div className="flex items-center gap-2 text-[9px] tracking-[0.3em] text-[#8E7A5E]">
        {icon} {label}
      </div>
      <div
        className={`mt-2 text-[12px] tracking-[0.12em] truncate ${
          tone === "cyan" ? "text-[#82D66B]" : "text-neutral-100"
        } ${mono ? "font-mono" : ""}`}
      >
        {value}
      </div>
    </div>
  );
}

function LogLine({
  ts,
  tone,
  msg,
}: {
  ts: string;
  tone: "ok" | "info" | "dim";
  msg: string;
}) {
  const color =
    tone === "ok"
      ? "text-[#82D66B]"
      : tone === "info"
      ? "text-[#F5A524]"
      : "text-[#8E7A5E]";
  return (
    <div className="flex gap-4">
      <span className="text-[#5E5040]">[{ts}]</span>
      <span className={color}>{msg}</span>
    </div>
  );
}

function Step({
  label,
  status,
}: {
  label: string;
  status: "done" | "active" | "pending";
}) {
  const dot =
    status === "done"
      ? "bg-[#82D66B] shadow-[0_0_6px_rgba(130,214,107,0.5)]"
      : status === "active"
      ? "bg-[#F5A524] animate-pulse shadow-[0_0_10px_rgba(245,165,36,0.7)]"
      : "bg-[#3A2C1D]";
  const text =
    status === "done"
      ? "text-[#C7AE86]"
      : status === "active"
      ? "text-[#F3E7D0]"
      : "text-[#8E7A5E]";
  const tag =
    status === "done" ? "DONE" : status === "active" ? "RUNNING" : "QUEUED";
  return (
    <div className="flex items-center justify-between border-b border-dashed border-[#18130D] pb-2 last:border-0">
      <div className="flex items-center gap-3">
        <span className={`size-2 rounded-full ${dot}`} />
        <span className={`text-[11px] tracking-[0.22em] ${text}`}>{label}</span>
      </div>
      <span className="text-[9px] tracking-[0.3em] text-[#8E7A5E]">{tag}</span>
    </div>
  );
}
