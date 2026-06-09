import { useState } from "react";
import { Folder, Cpu, Coffee, Globe, RefreshCw, LogOut, Save } from "lucide-react";
import { GridBackdrop, TopoBackdrop } from "./Backdrop";

export function SettingsScreen({ onLogout }: { onLogout: () => void }) {
  const [dir, setDir] = useState("C:/Users/Operator/AppData/BlockField");
  const [java, setJava] = useState("C:/Program Files/Java/jdk-21/bin/java.exe");
  const [ram, setRam] = useState(8);
  const [lang, setLang] = useState("EN-US");
  const [autoUpdate, setAutoUpdate] = useState(true);

  return (
    <div className="relative h-full w-full overflow-hidden bg-[#070604]">
      <TopoBackdrop />
      <GridBackdrop intensity={0.4} />

      <div className="relative h-full p-8 flex flex-col">
        <div className="flex items-center gap-3 mb-5">
          <span className="text-[10px] tracking-[0.4em] text-[#8E7A5E]">
            CONFIGURATION
          </span>
          <span className="h-px flex-1 bg-[#18130D]" />
          <span className="text-[10px] tracking-[0.4em] text-[#8E7A5E]">
            OPERATOR · KILO_7
          </span>
        </div>

        <div className="flex items-baseline gap-3">
          <h1 className="tracking-[0.06em] text-[34px] leading-none text-neutral-50">
            SETTINGS
          </h1>
          <span className="text-[10px] tracking-[0.32em] text-[#8E7A5E]">
            / LAUNCHER PREFERENCES
          </span>
        </div>

        <div className="mt-8 relative">
          <div className="absolute -inset-px border border-[#2A2116]/60 pointer-events-none" />
          <div className="relative border border-[#2A2116] bg-gradient-to-br from-[#11100D] to-[#0B0906]">
            <Group title="RUNTIME" code="ENV-001">
              <Setting
                icon={<Folder size={14} />}
                label="GAME DIRECTORY"
                hint="Location of all installed assets and player profiles."
              >
                <PathInput value={dir} onChange={setDir} />
              </Setting>
              <Setting
                icon={<Coffee size={14} />}
                label="JAVA RUNTIME"
                hint="Path to the Java executable used to launch the game."
              >
                <PathInput value={java} onChange={setJava} />
              </Setting>
              <Setting
                icon={<Cpu size={14} />}
                label="RAM ALLOCATION"
                hint={`${ram} GB allocated · recommended 6–12 GB`}
              >
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
                      onChange={(e) => setRam(parseInt(e.target.value))}
                      className="absolute inset-0 w-full opacity-0 cursor-pointer"
                    />
                  </div>
                  <span className="text-[12px] tracking-[0.18em] text-[#F3E7D0] w-16 text-right">
                    {ram} GB
                  </span>
                </div>
              </Setting>
            </Group>

            <Group title="LAUNCHER" code="LCH-002">
              <Setting
                icon={<Globe size={14} />}
                label="LANGUAGE"
                hint="Interface language for the launcher."
              >
                <div className="flex gap-1">
                  {["EN-US", "RU-RU", "DE-DE", "FR-FR"].map((l) => (
                    <button
                      key={l}
                      onClick={() => setLang(l)}
                      className={`h-9 px-4 text-[10px] tracking-[0.28em] border transition-colors ${
                        lang === l
                          ? "border-[#F5A524] bg-[#2A2116] text-[#F3E7D0]"
                          : "border-[#2A2116] bg-[#0B0906] text-[#C7AE86] hover:border-[#8A571C]"
                      }`}
                    >
                      {l}
                    </button>
                  ))}
                </div>
              </Setting>

              <Setting
                icon={<RefreshCw size={14} />}
                label="AUTO-UPDATE"
                hint="Automatically download and install new modpack versions."
              >
                <Toggle on={autoUpdate} onChange={setAutoUpdate} />
              </Setting>
            </Group>

            {/* Actions */}
            <div className="px-7 py-5 flex items-center justify-between border-t border-[#18130D] bg-[#0B0906]">
              <button
                onClick={onLogout}
                className="h-10 px-5 flex items-center gap-3 border border-[#3a2828] bg-[#1a0e0e] text-[#c98b8b] hover:border-[#7a3838] hover:text-[#e0a3a3] transition-colors"
              >
                <LogOut size={13} />
                <span className="text-[11px] tracking-[0.32em]">
                  LOGOUT OPERATOR
                </span>
              </button>
              <div className="flex items-center gap-3">
                <button className="h-10 px-5 border border-[#2A2116] text-[#C7AE86] hover:text-neutral-200 hover:border-[#3A2C1D] transition-colors text-[11px] tracking-[0.32em]">
                  RESET
                </button>
                <button
                  className="h-10 px-6 flex items-center gap-3 border border-[#F5A524]/40 bg-gradient-to-b from-[#2A2116] to-[#11100D] hover:border-[#F5A524] transition-colors"
                  style={{
                    boxShadow:
                      "0 0 24px -8px rgba(245,165,36,0.4), inset 0 0 0 1px rgba(245,165,36,0.08)",
                  }}
                >
                  <Save size={13} className="text-[#F3E7D0]" />
                  <span className="text-[11px] tracking-[0.32em] text-[#F3E7D0]">
                    SAVE CONFIGURATION
                  </span>
                </button>
              </div>
            </div>
          </div>
        </div>
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
  children: React.ReactNode;
}) {
  return (
    <div className="border-b border-[#18130D] last:border-b-0">
      <div className="px-7 pt-5 pb-3 flex items-center gap-3">
        <span className="size-1.5 bg-[#F5A524]" />
        <span className="text-[10px] tracking-[0.32em] text-[#F3E7D0]">
          {title}
        </span>
        <span className="h-px flex-1 bg-[#18130D]" />
        <span className="text-[9px] tracking-[0.28em] text-[#5E5040]">
          {code}
        </span>
      </div>
      <div className="px-7 pb-2">{children}</div>
    </div>
  );
}

function Setting({
  icon,
  label,
  hint,
  children,
}: {
  icon: React.ReactNode;
  label: string;
  hint: string;
  children: React.ReactNode;
}) {
  return (
    <div className="grid grid-cols-[280px_1fr] gap-8 py-4 border-b border-dashed border-[#18130D] last:border-b-0">
      <div className="flex gap-3">
        <div className="mt-0.5 size-7 grid place-items-center border border-[#2A2116] bg-[#0B0906] text-[#F5A524]">
          {icon}
        </div>
        <div className="flex flex-col">
          <span className="text-[11px] tracking-[0.22em] text-neutral-100">
            {label}
          </span>
          <span className="text-[11px] leading-snug text-[#8E7A5E] mt-1">
            {hint}
          </span>
        </div>
      </div>
      <div className="flex items-center">{children}</div>
    </div>
  );
}

function PathInput({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <div className="flex w-full">
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="flex-1 h-10 border border-[#2A2116] bg-[#0B0906] px-3 text-[12px] font-mono text-neutral-200 outline-none focus:border-[#F5A524]/60 transition-colors"
      />
      <button className="h-10 px-4 border border-l-0 border-[#2A2116] bg-[#11100D] text-[10px] tracking-[0.3em] text-[#C7AE86] hover:text-[#F3E7D0] hover:border-[#8A571C] transition-colors">
        BROWSE
      </button>
    </div>
  );
}

function Toggle({
  on,
  onChange,
}: {
  on: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      onClick={() => onChange(!on)}
      className={`relative h-7 w-14 border transition-colors ${
        on
          ? "border-[#F5A524]/60 bg-[#2A2116]"
          : "border-[#2A2116] bg-[#0B0906]"
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
        className={`absolute -bottom-5 right-0 text-[9px] tracking-[0.3em] ${
          on ? "text-[#F5A524]" : "text-[#8E7A5E]"
        }`}
      >
        {on ? "ENABLED" : "DISABLED"}
      </span>
    </button>
  );
}
