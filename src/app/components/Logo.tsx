type LogoProps = {
  size?: number;
  withWordmark?: boolean;
  className?: string;
};

export function Logo({ size = 28, withWordmark = false, className = "" }: LogoProps) {
  return (
    <div className={`flex items-center gap-2.5 ${className}`}>
      <svg
        width={size}
        height={size}
        viewBox="0 0 32 32"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
      >
        <path
          d="M4 7L16 2L28 7V25L16 30L4 25V7Z"
          stroke="#F5A524"
          strokeWidth="1.4"
          strokeLinejoin="round"
        />
        <path
          d="M4 7L16 12L28 7"
          stroke="#F5A524"
          strokeWidth="1.4"
          strokeLinejoin="round"
        />
        <path d="M16 12V30" stroke="#F5A524" strokeWidth="1.4" />
        <path d="M10 9.5L22 14.5" stroke="#8A571C" strokeWidth="1" />
        <rect x="14" y="16" width="4" height="4" fill="#F5A524" />
      </svg>
      {withWordmark && (
        <div className="flex flex-col leading-none">
          <span className="tracking-[0.22em] text-[13px] text-neutral-100">
            BLOCKFIELD
          </span>
          <span className="tracking-[0.32em] text-[9px] text-[#C7AE86] mt-1">
            TACTICAL OPS
          </span>
        </div>
      )}
    </div>
  );
}
