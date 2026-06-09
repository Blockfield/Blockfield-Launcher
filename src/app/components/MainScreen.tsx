import { Play, Wifi, Users, Activity, ShieldCheck, Flag, Swords, Truck, Crosshair, ChevronRight, RefreshCw } from "lucide-react";
import { GridBackdrop, TopoBackdrop } from "./Backdrop";

export function MainScreen({ onPlay }: { onPlay: () => void }) {
  return (
    <div className="relative h-full w-full overflow-hidden bg-[#070604]">
      <TopoBackdrop />
      <GridBackdrop intensity={0.5} />

      <div className="relative h-full grid grid-cols-[1fr_360px] gap-0">
        {/* Hero / Command Panel */}
        <section className="relative p-8 flex flex-col">
          <div className="flex items-center gap-3 mb-5">
            <span className="text-[10px] tracking-[0.4em] text-[#8E7A5E]">
              OPERATION
            </span>
            <span className="h-px flex-1 bg-[#18130D]" />
            <span className="flex items-center gap-2 text-[10px] tracking-[0.4em] text-[#82D66B]">
              <span className="size-1.5 rounded-full bg-[#82D66B] animate-pulse shadow-[0_0_6px_rgba(130,214,107,0.7)]" />
              ACTIVE
            </span>
          </div>

          <div className="flex items-start justify-between gap-6">
            <div className="max-w-[560px]">
              <div className="flex items-baseline gap-3 mb-2">
                <h1 className="tracking-[0.06em] text-[34px] leading-none text-neutral-50">
                  IRON FRONT
                </h1>
                <span className="text-[10px] tracking-[0.32em] text-[#8E7A5E]">
                  / SEASON 01
                </span>
              </div>
              <p className="text-[13px] leading-relaxed text-[#C7AE86] max-w-[520px]">
                Large-scale tactical PvP across contested terrain. Capture
                strategic points, coordinate with your squad, and command
                armored vehicles to break enemy lines.
              </p>
            </div>

            <ServerStatus />
          </div>

          {/* PLAY zone */}
          <div className="mt-8 relative">
            <div className="absolute -inset-px border border-[#F5A524]/20 pointer-events-none" />
            <div className="relative border border-[#2A2116] bg-gradient-to-br from-[#11100D] via-[#11100D] to-[#0B0906] p-6">
              <div className="flex items-center justify-between gap-6">
                <div className="flex items-center gap-5">
                  <button
                    onClick={onPlay}
                    className="group relative h-[88px] w-[260px] overflow-hidden border border-[#F5A524]/50 bg-gradient-to-b from-[#2A2116] to-[#11100D] hover:border-[#F5A524] transition-all"
                    style={{
                      boxShadow:
                        "inset 0 0 0 1px rgba(245,165,36,0.1), 0 0 40px -8px rgba(245,165,36,0.45)",
                    }}
                  >
                    {/* angled corners */}
                    <span className="absolute top-0 left-0 w-3 h-3 border-l border-t border-[#F5A524]" />
                    <span className="absolute top-0 right-0 w-3 h-3 border-r border-t border-[#F5A524]" />
                    <span className="absolute bottom-0 left-0 w-3 h-3 border-l border-b border-[#F5A524]" />
                    <span className="absolute bottom-0 right-0 w-3 h-3 border-r border-b border-[#F5A524]" />
                    <span className="absolute inset-0 bg-[#F5A524]/0 group-hover:bg-[#F5A524]/10 transition-colors" />
                    <span className="relative h-full flex items-center justify-center gap-4">
                      <Play size={22} className="text-[#F3E7D0] fill-[#F3E7D0]" />
                      <span className="flex flex-col items-start leading-none">
                        <span className="tracking-[0.4em] text-[20px] text-[#F3E7D0]">
                          DEPLOY
                        </span>
                        <span className="tracking-[0.32em] text-[10px] text-[#C7AE86] mt-1.5">
                          ENTER BATTLEFIELD
                        </span>
                      </span>
                    </span>
                  </button>

                  <div className="flex flex-col gap-2 pl-2">
                    <Stat label="MODPACK" value="UP TO DATE" tone="ok" />
                    <Stat label="AUTH" value="VERIFIED" tone="ok" />
                    <Stat label="QUEUE" value="NONE" tone="muted" />
                  </div>
                </div>

                <div className="flex items-center gap-6 text-right">
                  <Metric icon={<Users size={14} />} label="OPERATORS" value="142" sub="/ 200" />
                  <span className="h-10 w-px bg-[#18130D]" />
                  <Metric icon={<Activity size={14} />} label="PING" value="28" sub="MS" />
                  <span className="h-10 w-px bg-[#18130D]" />
                  <Metric icon={<Wifi size={14} />} label="REGION" value="EU-W" sub="FRA" />
                </div>
              </div>
            </div>
          </div>

          {/* Briefing + Modpack row */}
          <div className="mt-6 grid grid-cols-[1.4fr_1fr] gap-px bg-[#18130D] border border-[#2A2116]">
            <div className="bg-[#0B0906] p-6">
              <SectionHeader label="MISSION BRIEFING" code="BRF-001" />
              <div className="grid grid-cols-2 gap-x-6 gap-y-4 mt-4">
                <Feature icon={<Flag size={14} />} title="CAPTURE POINTS" desc="Dynamic objective control across multiple sectors." />
                <Feature icon={<Swords size={14} />} title="6 CLASSES" desc="Assault, Recon, Engineer, Medic, Support, Pilot." />
                <Feature icon={<Truck size={14} />} title="ARMORED VEHICLES" desc="Tanks, APCs, light recon and air transport." />
                <Feature icon={<Crosshair size={14} />} title="TACTICAL BATTLES" desc="Squad-based 64v64 persistent warfare." />
              </div>
            </div>

            <div className="bg-[#0B0906] p-6 flex flex-col">
              <SectionHeader label="MODPACK STATUS" code="PKG-0142" />
              <div className="mt-4 flex flex-col gap-3 flex-1">
                <Row label="INSTALLED" value="0.1.42" />
                <Row label="LATEST" value="0.1.42" highlight />
                <Row label="SIZE" value="2.1 GB" />
                <Row label="AUTO-UPDATE" value="ENABLED" highlight />
              </div>
              <button className="mt-4 h-9 border border-[#2A2116] hover:border-[#8A571C] text-[10px] tracking-[0.3em] text-[#C7AE86] hover:text-[#F3E7D0] flex items-center justify-center gap-2 transition-colors">
                <RefreshCw size={12} />
                CHECK FOR UPDATES
              </button>
            </div>
          </div>
        </section>

        {/* Side rail */}
        <aside className="relative border-l border-[#18130D] bg-[#0B0906]/80 p-6 flex flex-col">
          <SectionHeader label="FIELD REPORT" code="OPS-LOG" />

          <div className="mt-5 flex-1 flex flex-col gap-px bg-[#18130D] border border-[#2A2116]">
            <FeedItem
              tag="PATCH"
              tone="green"
              date="06.07"
              title="0.1.42 — Vehicle Balance"
              body="Reduced T-72 reload by 0.4s. New recon drone for Scout class. Two map reworks deployed to rotation."
            />
            <FeedItem
              tag="EVENT"
              tone="cyan"
              date="06.05"
              title="Operation Blackridge"
              body="48-hour persistent campaign begins Friday 19:00 UTC. Double XP across all classes."
            />
            <FeedItem
              tag="OPS"
              tone="steel"
              date="06.02"
              title="Anti-cheat upgrade"
              body="New kernel-level driver active. Expect lower false-positives and tighter enforcement."
            />
          </div>

          <button className="mt-4 h-9 text-[10px] tracking-[0.3em] text-[#8E7A5E] hover:text-[#F3E7D0] flex items-center justify-between border-t border-[#18130D] pt-4">
            <span>VIEW FULL OPERATIONS LOG</span>
            <ChevronRight size={12} />
          </button>
        </aside>
      </div>
    </div>
  );
}

function ServerStatus() {
  return (
    <div className="border border-[#2A2116] bg-[#0B0906] px-4 py-3 min-w-[220px]">
      <div className="flex items-center justify-between">
        <span className="text-[10px] tracking-[0.3em] text-[#8E7A5E]">
          SERVER
        </span>
        <span className="flex items-center gap-1.5 text-[10px] tracking-[0.3em] text-[#82D66B]">
          <span className="size-1.5 rounded-full bg-[#82D66B] shadow-[0_0_8px_rgba(130,214,107,0.8)]" />
          ONLINE
        </span>
      </div>
      <div className="mt-2 flex items-center gap-2">
        <ShieldCheck size={14} className="text-[#F5A524]" />
        <span className="tracking-[0.18em] text-[13px] text-neutral-100">
          BLOCKFIELD · PRIMARY
        </span>
      </div>
      <div className="mt-2 text-[10px] tracking-[0.22em] text-[#8E7A5E]">
        play.blockfield.gg:25565
      </div>
    </div>
  );
}

function Stat({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone: "ok" | "muted" | "warn";
}) {
  const color =
    tone === "ok"
      ? "text-[#82D66B]"
      : tone === "warn"
      ? "text-[#F5A524]"
      : "text-[#C7AE86]";
  return (
    <div className="flex items-center gap-3">
      <span className="text-[9px] tracking-[0.3em] text-[#8E7A5E] w-16">
        {label}
      </span>
      <span className={`text-[10px] tracking-[0.3em] ${color}`}>
        ▸ {value}
      </span>
    </div>
  );
}

function Metric({
  icon,
  label,
  value,
  sub,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
  sub: string;
}) {
  return (
    <div className="flex flex-col items-end gap-1">
      <span className="flex items-center gap-1.5 text-[9px] tracking-[0.3em] text-[#8E7A5E]">
        {icon} {label}
      </span>
      <span className="tracking-[0.08em] text-[22px] leading-none text-neutral-50">
        {value}
        <span className="text-[10px] tracking-[0.3em] text-[#8E7A5E] ml-1">
          {sub}
        </span>
      </span>
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

function Feature({
  icon,
  title,
  desc,
}: {
  icon: React.ReactNode;
  title: string;
  desc: string;
}) {
  return (
    <div className="flex gap-3">
      <div className="mt-0.5 size-7 grid place-items-center border border-[#2A2116] bg-[#11100D] text-[#F5A524]">
        {icon}
      </div>
      <div className="flex flex-col gap-1">
        <span className="text-[11px] tracking-[0.22em] text-neutral-100">
          {title}
        </span>
        <span className="text-[11px] leading-snug text-[#8E7A5E]">{desc}</span>
      </div>
    </div>
  );
}

function Row({
  label,
  value,
  highlight,
}: {
  label: string;
  value: string;
  highlight?: boolean;
}) {
  return (
    <div className="flex items-center justify-between border-b border-dashed border-[#18130D] pb-2 last:border-0">
      <span className="text-[10px] tracking-[0.3em] text-[#8E7A5E]">
        {label}
      </span>
      <span
        className={`text-[11px] tracking-[0.22em] ${
          highlight ? "text-[#F5A524]" : "text-neutral-100"
        }`}
      >
        {value}
      </span>
    </div>
  );
}

function FeedItem({
  tag,
  tone,
  date,
  title,
  body,
}: {
  tag: string;
  tone: "green" | "cyan" | "steel";
  date: string;
  title: string;
  body: string;
}) {
  const tagColor =
    tone === "green"
      ? "text-[#F5A524] border-[#8A571C]"
      : tone === "cyan"
      ? "text-[#82D66B] border-[#3a5a30]"
      : "text-[#C7AE86] border-[#3A2C1D]";
  return (
    <div className="bg-[#0B0906] p-4 flex flex-col gap-2 hover:bg-[#11100D] transition-colors cursor-pointer">
      <div className="flex items-center justify-between">
        <span className={`text-[9px] tracking-[0.3em] border px-1.5 py-0.5 ${tagColor}`}>
          {tag}
        </span>
        <span className="text-[10px] tracking-[0.24em] text-[#5E5040]">
          {date}
        </span>
      </div>
      <span className="text-[12px] tracking-[0.04em] text-neutral-100">
        {title}
      </span>
      <span className="text-[11px] leading-snug text-[#8E7A5E]">{body}</span>
    </div>
  );
}
