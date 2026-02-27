import { Link } from "react-router-dom";
import { ScrollText } from "lucide-react";

export default function TransactionHash({
  hash,
  truncate = true,
}: {
  hash: string;
  truncate?: boolean;
}) {
  return (
    <Link
      to={`/tx/${hash}`}
      className="inline-flex items-center gap-1 font-mono text-xs text-gray-700 hover:underline"
    >
      <ScrollText className="size-3.5 shrink-0 text-gray-400" />
      {truncate ? (
        <span className="inline-block max-w-[10ch] overflow-hidden text-ellipsis whitespace-nowrap align-bottom">{hash}</span>
      ) : <span className="break-all">{hash}</span>}
    </Link>
  );
}
