import { Link } from "react-router-dom";

export default function BlockHash({ hash }: { hash: string }) {
  return (
    <Link to={`/block/${hash}`} className="font-mono text-xs break-all text-blue-600 hover:underline">
      {hash}
    </Link>
  );
}
