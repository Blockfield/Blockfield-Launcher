export function Tick({ className = "" }: { className?: string }) {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 14 14"
      className={`absolute ${className}`}
      fill="none"
      aria-hidden
    >
      <path d="M0 0H6" stroke="#F5A524" strokeWidth="1" />
      <path d="M0 0V6" stroke="#F5A524" strokeWidth="1" />
    </svg>
  );
}
