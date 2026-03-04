import { Link } from "react-router-dom";

export default function BlockHeight({ height }: { height: number | null }) {
  if (height == null) return <span className="text-gray-400">—</span>;
  return (
    <Link to={`/block/${height}`} className="text-blue-600 hover:underline">
      {height.toLocaleString()}
    </Link>
  );
}
