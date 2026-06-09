import type { ReactNode } from "react";
import { Logo } from "./Logo";
import { Settings, LogOut, Gamepad2, Download, LifeBuoy } from "lucide-react";

type Screen = "main" | "update" | "settings";

export function Shell({
  active,
  onNavigate,
  onLogout,
  children,
}: {
  active: Screen;
  onNavigate: (s: Screen) => void;
  onLogout: () => void;
  children: ReactNode;
}) {
  return (
    <div className="relative h-full w-full bg-[#070604] flex flex-col">
      {/* Top bar */}
      <header className="h-14 shrink-0 border-b border-[#18130D] bg-[#0B0906] flex items-center justify-between px-5">
        <div className="flex items-center gap-8">
          <Logo size={26} withWordmark />
          <nav className="flex items-center gap-1">
            <NavItem
              icon={<Gamepad2 size={13} />}
              label="DEPLOY"
              active={active === "main"}
              onClick={() => onNavigate("main")}
            />
            <NavItem
              icon={<Download size={13} />}
              label="UPDATES"
              active={active === "update"}
              onClick={() => onNavigate("update")}
            />
            <NavItem
              icon={<Settings size={13} />}
              label="SETTINGS"
              active={active === "settings"}
              onClick={() => onNavigate("settings")}
            />
          </nav>
        </div>

        <div className="flex items-center gap-4">
          <div className="flex items-center gap-3 px-3 h-9 border border-[#2A2116] bg-[#11100D]">
            <div className="relative">
              <div className="size-6 bg-gradient-to-br from-[#8A571C] to-[#2A2116] grid place-items-center text-[10px] text-[#F3E7D0] tracking-widest">
                K7
              </div>
              <span className="absolute -bottom-0.5 -right-0.5 size-1.5 rounded-full bg-[#82D66B] ring-2 ring-[#11100D]" />
            </div>
            <div className="flex flex-col leading-none">
              <span className="text-[11px] tracking-widest text-neutral-100">
                KILO_7
              </span>
              <span className="text-[9px] tracking-[0.2em] text-[#8E7A5E] mt-0.5">
                RANK · SERGEANT
              </span>
            </div>
          </div>
          <button
            onClick={onLogout}
            className="h-9 w-9 grid place-items-center border border-[#2A2116] bg-[#11100D] text-[#8E7A5E] hover:text-[#F3E7D0] hover:border-[#8A571C] transition-colors"
            title="Logout"
          >
            <LogOut size={13} />
          </button>
        </div>
      </header>

      <div className="flex-1 relative overflow-hidden">{children}</div>

      {/* Footer */}
      <footer className="h-8 shrink-0 border-t border-[#18130D] bg-[#0B0906] flex items-center justify-between px-5 text-[10px] tracking-[0.24em] text-[#5E5040]">
        <div className="flex items-center gap-4">
          <span>LAUNCHER v0.4.2</span>
          <span className="h-3 w-px bg-[#18130D]" />
          <span>IP · play.blockfield.gg:25565</span>
        </div>
        <div className="flex items-center gap-4">
          <a className="flex items-center gap-1.5 hover:text-[#F5A524] cursor-pointer">
            <LifeBuoy size={11} /> SUPPORT
          </a>
          <span className="h-3 w-px bg-[#18130D]" />
          <span className="flex items-center gap-1.5">
            <span className="size-1.5 rounded-full bg-[#82D66B] shadow-[0_0_6px_rgba(130,214,107,0.6)]" />
            NETWORK NOMINAL
          </span>
        </div>
      </footer>
    </div>
  );
}

function NavItem({
  icon,
  label,
  active,
  onClick,
}: {
  icon: ReactNode;
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={`relative h-9 px-3.5 flex items-center gap-2 text-[10px] tracking-[0.28em] transition-colors ${
        active
          ? "text-[#F3E7D0]"
          : "text-[#8E7A5E] hover:text-neutral-200"
      }`}
    >
      {icon}
      {label}
      {active && (
        <span className="absolute left-2 right-2 -bottom-px h-px bg-[#F5A524]" />
      )}
    </button>
  );
}
