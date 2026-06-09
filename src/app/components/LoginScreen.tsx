import { useState } from "react";
import { Logo } from "./Logo";
import { GridBackdrop, TopoBackdrop, CornerTicks } from "./Backdrop";
import { ArrowRight, Lock, User } from "lucide-react";

export function LoginScreen({ onSignIn }: { onSignIn: () => void }) {
  const [remember, setRemember] = useState(true);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");

  return (
    <div className="relative h-full w-full bg-[#070604] overflow-hidden">
      <TopoBackdrop />
      <GridBackdrop intensity={0.6} />

      {/* Side legend */}
      <div className="absolute left-5 top-5 text-[10px] tracking-[0.3em] text-[#5E5040] flex flex-col gap-2">
        <span>SECTOR 07 — NORTH RIDGE</span>
        <span>LAT 47.3829° / LON 19.0402°</span>
      </div>
      <div className="absolute right-5 top-5 text-[10px] tracking-[0.3em] text-[#5E5040] flex flex-col items-end gap-2">
        <span>BUILD 0.4.2 — STABLE</span>
        <span className="flex items-center gap-2">
          <span className="size-1.5 rounded-full bg-[#82D66B] animate-pulse shadow-[0_0_6px_rgba(130,214,107,0.7)]" />
          AUTH SERVICE: ONLINE
        </span>
      </div>

      <div className="absolute bottom-5 left-5 right-5 flex items-end justify-between text-[10px] tracking-[0.3em] text-[#5E5040]">
        <span>“DEPLOY. CAPTURE. DOMINATE.”</span>
        <span>© 2026 BLOCKFIELD COMMAND</span>
      </div>

      {/* Center form */}
      <div className="h-full w-full grid place-items-center">
        <div className="relative w-[420px]">
          <CornerTicks />
          <div className="border border-[#2A2116] bg-[#11100D]/90 backdrop-blur-sm">
            <div className="px-9 pt-8 pb-7">
              <div className="flex flex-col items-center gap-5">
                <Logo size={44} />
                <div className="flex flex-col items-center gap-1.5">
                  <div className="tracking-[0.4em] text-[11px] text-[#C7AE86]">
                    BLOCKFIELD
                  </div>
                  <h2 className="tracking-[0.18em] text-neutral-50">
                    LAUNCHER
                  </h2>
                </div>
                <div className="h-px w-16 bg-gradient-to-r from-transparent via-[#8A571C] to-transparent" />
                <p className="text-[11px] tracking-[0.18em] text-[#8E7A5E]">
                  OPERATOR AUTHENTICATION REQUIRED
                </p>
              </div>

              <div className="mt-6 flex flex-col gap-3.5">
                <Field
                  icon={<User size={13} />}
                  label="CALLSIGN / EMAIL"
                  value={username}
                  onChange={setUsername}
                  placeholder="operator@blockfield.gg"
                />
                <Field
                  icon={<Lock size={13} />}
                  label="ACCESS KEY"
                  value={password}
                  onChange={setPassword}
                  placeholder="••••••••••••"
                  type="password"
                />

                <div className="flex items-center justify-between mt-1">
                  <label className="flex items-center gap-2 cursor-pointer group">
                    <span
                      onClick={() => setRemember(!remember)}
                      className={`size-3.5 border flex items-center justify-center transition-colors ${
                        remember
                          ? "border-[#F5A524] bg-[#F5A524]/15"
                          : "border-[#3A2C1D] bg-transparent"
                      }`}
                    >
                      {remember && (
                        <span className="size-1.5 bg-[#F5A524]" />
                      )}
                    </span>
                    <span className="text-[10px] tracking-[0.22em] text-[#C7AE86] group-hover:text-neutral-200">
                      REMEMBER OPERATOR
                    </span>
                  </label>
                  <button className="text-[10px] tracking-[0.22em] text-[#8E7A5E] hover:text-[#F5A524]">
                    FORGOT KEY?
                  </button>
                </div>

                <button
                  onClick={onSignIn}
                  className="relative mt-3 h-11 group overflow-hidden border border-[#F5A524]/40 bg-gradient-to-b from-[#2A2116] to-[#11100D] hover:border-[#F5A524] transition-all"
                  style={{
                    boxShadow:
                      "inset 0 0 0 1px rgba(245,165,36,0.08), 0 0 24px -8px rgba(245,165,36,0.4)",
                  }}
                >
                  <span className="absolute inset-0 bg-[#F5A524]/0 group-hover:bg-[#F5A524]/10 transition-colors" />
                  <span className="relative flex items-center justify-center gap-3 text-[12px] tracking-[0.4em] text-[#F3E7D0]">
                    SIGN IN
                    <ArrowRight size={14} />
                  </span>
                </button>
              </div>
            </div>

            <div className="border-t border-[#18130D] px-6 py-3 flex items-center justify-between bg-[#0B0906]">
              <div className="flex items-center gap-2">
                <span className="size-1.5 rounded-full bg-[#F5A524]" />
                <span className="text-[10px] tracking-[0.22em] text-[#8E7A5E]">
                  LAUNCHER READY · v0.4.2
                </span>
              </div>
              <span className="text-[10px] tracking-[0.22em] text-[#5E5040]">
                EU-WEST · 28ms
              </span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function Field({
  label,
  value,
  onChange,
  placeholder,
  type = "text",
  icon,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  type?: string;
  icon?: React.ReactNode;
}) {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="text-[10px] tracking-[0.28em] text-[#8E7A5E]">
        {label}
      </span>
      <div className="flex items-center gap-2 h-10 border border-[#2A2116] bg-[#0B0906] px-3 focus-within:border-[#F5A524]/60 transition-colors">
        <span className="text-[#8E7A5E]">{icon}</span>
        <span className="h-4 w-px bg-[#2A2116]" />
        <input
          type={type}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={placeholder}
          className="flex-1 bg-transparent outline-none text-[13px] text-neutral-100 placeholder:text-[#5E5040] tracking-wide"
        />
      </div>
    </label>
  );
}
