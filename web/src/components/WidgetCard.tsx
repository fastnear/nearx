import type { ReactNode } from "react";

export default function WidgetCard({
  icon,
  children,
  className = "",
}: {
  icon?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={`mb-4 flex items-start gap-3 rounded-lg border bg-surface px-4 py-3 text-sm ${className}`}
    >
      {icon && <span className="mt-0.5 shrink-0">{icon}</span>}
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}
