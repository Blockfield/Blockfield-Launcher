import { LogoMark } from "./icons";

type LogoProps = {
  size?: number;
  withWordmark?: boolean;
  className?: string;
};

export function Logo({ size = 28, withWordmark = false, className = "" }: LogoProps) {
  return (
    <div className={`flex items-center gap-2.5 ${className}`}>
      <LogoMark size={size} />
      {withWordmark && (
        <div className="flex flex-col leading-none">
          <span className="tracking-[0.22em] text-[13px] text-neutral-100 uppercase">
            BLOCKFIELD
          </span>
          <span className="tracking-[0.18em] text-[9px] text-[#C7AE86] mt-1 uppercase">
            TACTICAL OPS
          </span>
        </div>
      )}
    </div>
  );
}
